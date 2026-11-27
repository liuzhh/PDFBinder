//! PDF 合并：把多个文档按给定顺序拼成一个。
//!
//! 做法参考 lopdf 官方 merge 示例：所有对象重新编号后放进同一张对象表，
//! 丢弃原有的页面树与书签，最后用收集到的页面按顺序重建一棵扁平页面树。

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Document, Error, Object, ObjectId};

/// 读取 PDF 并返回页数，用于添加文件时的校验与统计。
pub fn page_count(path: &Path) -> Result<u32, String> {
    let doc = Document::load(path).map_err(|err| format!("无法读取：{err}"))?;
    let pages = doc.get_pages().len() as u32;
    if pages == 0 {
        return Err("文件中没有页面".to_owned());
    }
    Ok(pages)
}

/// 按 `paths` 的顺序合并，写到 `output`，返回结果页数。
///
/// 写入后会重新打开产物校验页数，避免静默产出损坏文件。
pub fn merge_to_file(paths: &[PathBuf], output: &Path) -> Result<u32, String> {
    if paths.is_empty() {
        return Err("没有可合并的文件".to_owned());
    }

    let mut documents = Vec::with_capacity(paths.len());
    let mut expected = 0u32;

    for path in paths {
        let mut doc =
            Document::load(path).map_err(|err| format!("{}：读取失败（{err}）", path.display()))?;
        if doc.is_encrypted() {
            // 大多数 PDF 只是设置了权限密码，空密码即可打开；真正的打开密码在这里会失败。
            doc.decrypt("")
                .map_err(|_| format!("{}：文件已加密，暂不支持合并", path.display()))?;
        }
        let pages = doc.get_pages().len() as u32;
        if pages == 0 {
            return Err(format!("{}：文件中没有页面", path.display()));
        }
        expected += pages;
        documents.push(doc);
    }

    let mut merged = merge(documents)?;
    merged
        .save(output)
        .map_err(|err| format!("写入 {} 失败：{err}", output.display()))?;

    let written = Document::load(output).map_err(|err| format!("写出的文件无法重新打开：{err}"))?;
    let actual = written.get_pages().len() as u32;
    if actual != expected {
        return Err(format!(
            "页数校验失败：应为 {expected} 页，实际 {actual} 页"
        ));
    }

    Ok(actual)
}

fn merge(documents: Vec<Document>) -> Result<Document, String> {
    let mut merged = Document::with_version("1.5");
    let mut next_id = 1u32;
    let mut pages: Vec<(ObjectId, Dictionary)> = Vec::new();
    let mut objects: BTreeMap<ObjectId, Object> = BTreeMap::new();

    for mut doc in documents {
        // 合并后会丢掉原来的 Pages 节点，所以先把 MediaBox 等继承属性固化到每一页上。
        for page_id in doc.get_pages().into_values() {
            if let Ok(page) = materialize_page(&doc, page_id) {
                doc.objects.insert(page_id, Object::Dictionary(page));
            }
        }

        // 重新编号，让每个文档的对象 ID 互不重叠，之后才能共存于同一张对象表。
        doc.renumber_objects_with(next_id);
        next_id = doc.max_id + 1;

        for page_id in doc.get_pages().into_values() {
            if let Ok(Object::Dictionary(page)) = doc.get_object(page_id) {
                pages.push((page_id, page.clone()));
            }
        }

        objects.extend(doc.objects);
    }

    if pages.is_empty() {
        return Err("合并结果没有任何页面".to_owned());
    }

    let mut catalog: Option<(ObjectId, Dictionary)> = None;
    let mut pages_node: Option<(ObjectId, Dictionary)> = None;

    for (id, object) in objects {
        match object.type_name().unwrap_or(b"") {
            b"Catalog" => {
                if catalog.is_none()
                    && let Ok(dict) = object.as_dict()
                {
                    catalog = Some((id, dict.clone()));
                }
            }
            b"Pages" => {
                if pages_node.is_none()
                    && let Ok(dict) = object.as_dict()
                {
                    pages_node = Some((id, dict.clone()));
                }
            }
            // 页面稍后统一挂到新页面上；书签暂不支持，直接丢弃。
            b"Page" | b"Outlines" | b"Outline" => {}
            _ => {
                merged.objects.insert(id, object);
            }
        }
    }

    let (pages_id, mut pages_dict) =
        pages_node.ok_or_else(|| "源文件中找不到页面树（Pages）".to_owned())?;
    let (catalog_id, mut catalog_dict) =
        catalog.ok_or_else(|| "源文件中找不到文档目录（Catalog）".to_owned())?;

    let mut kids = Vec::with_capacity(pages.len());
    for (page_id, mut page) in pages {
        page.set("Parent", pages_id);
        kids.push(Object::Reference(page_id));
        merged.objects.insert(page_id, Object::Dictionary(page));
    }

    // 新页面树只保留结构字段，尺寸/资源已固化到各页。
    let _ = pages_dict.remove(b"Parent");
    for key in ["MediaBox", "CropBox", "Resources", "Rotate"] {
        let _ = pages_dict.remove(key.as_bytes());
    }
    pages_dict.set("Count", kids.len() as i64);
    pages_dict.set("Kids", kids);
    merged
        .objects
        .insert(pages_id, Object::Dictionary(pages_dict));

    catalog_dict.set("Pages", pages_id);
    let _ = catalog_dict.remove(b"Outlines");
    let _ = catalog_dict.remove(b"PageMode");
    merged
        .objects
        .insert(catalog_id, Object::Dictionary(catalog_dict));

    merged.trailer.set("Root", catalog_id);
    merged.max_id = merged.objects.keys().map(|id| id.0).max().unwrap_or(0) + 1;

    Ok(merged)
}

/// 把页面从父节点继承的属性（MediaBox / CropBox / Resources / Rotate）复制到页面字典。
fn materialize_page(doc: &Document, page_id: ObjectId) -> Result<Dictionary, Error> {
    let mut page = doc.get_dictionary(page_id)?.clone();

    let media_box = inherited_attribute(doc, page_id, b"MediaBox")?
        .ok_or_else(|| Error::DictKey("MediaBox".to_owned()))?;
    let crop_box =
        inherited_attribute(doc, page_id, b"CropBox")?.unwrap_or_else(|| media_box.clone());
    let resources = inherited_attribute(doc, page_id, b"Resources")?
        .unwrap_or_else(|| Object::Dictionary(Dictionary::new()));
    let rotate = inherited_attribute(doc, page_id, b"Rotate")?.unwrap_or(Object::Integer(0));

    page.set("MediaBox", media_box);
    page.set("CropBox", crop_box);
    page.set("Resources", resources);
    page.set("Rotate", rotate);

    Ok(page)
}

/// 沿 Parent 链向上查找页面属性，返回最近的有效值。
fn inherited_attribute(
    doc: &Document,
    page_id: ObjectId,
    key: &[u8],
) -> Result<Option<Object>, Error> {
    let mut current = page_id;
    let mut visited = HashSet::new();

    loop {
        if !visited.insert(current) {
            return Err(Error::ReferenceCycle(current));
        }

        let node = doc.get_dictionary(current)?;
        if let Ok(value) = node.get(key) {
            match doc.dereference(value) {
                Ok((_, Object::Null)) | Err(Error::ObjectNotFound(_)) => {}
                Ok(_) => return Ok(Some(value.clone())),
                Err(err) => return Err(err),
            }
        }

        match node.get(b"Parent") {
            Err(_) | Ok(Object::Null) => return Ok(None),
            Ok(parent) => current = parent.as_reference()?,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Stream;
    use lopdf::content::{Content, Operation};
    use lopdf::dictionary;

    /// 生成一个每页宽度不同的文档，便于校验合并后的页序。
    fn sample_pdf(widths: &[i64]) -> Document {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();

        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        });

        let mut kids = Vec::new();
        for width in widths {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![72.into(), 700.into()]),
                    Operation::new("Tj", vec![Object::string_literal(format!("page {width}"))]),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
                "MediaBox" => vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(*width),
                    Object::Integer(842),
                ],
            });
            kids.push(page_id);
        }

        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids.into_iter().map(Object::Reference).collect::<Vec<_>>(),
                "Count" => count,
            }),
        );

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc
    }

    fn merged_widths(output: &Path) -> Vec<i64> {
        let doc = Document::load(output).unwrap();
        doc.get_pages()
            .into_values()
            .map(|page_id| {
                let page = doc.get_dictionary(page_id).unwrap();
                page.get_deref(b"MediaBox", &doc)
                    .unwrap()
                    .as_array()
                    .unwrap()[2]
                    .as_i64()
                    .unwrap()
            })
            .collect()
    }

    #[test]
    fn merges_in_requested_order() {
        let dir = std::env::temp_dir().join(format!("pdfbinder-merge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first.pdf");
        let second = dir.join("second.pdf");
        let output = dir.join("merged.pdf");

        sample_pdf(&[100, 110]).save(&first).unwrap();
        sample_pdf(&[200]).save(&second).unwrap();

        let pages = merge_to_file(&[first.clone(), second.clone()], &output).unwrap();
        assert_eq!(pages, 3);
        assert_eq!(merged_widths(&output), vec![100, 110, 200]);

        // 页面内容也必须跟着页面一起走，而不是错位到别的页。
        let merged = Document::load(&output).unwrap();
        for (number, width) in [(1u32, 100), (2, 110), (3, 200)] {
            let text = merged.extract_text(&[number]).unwrap();
            assert!(
                text.contains(&format!("page {width}")),
                "第 {number} 页内容不对：{text:?}"
            );
        }

        // 顺序由调用方决定，倒过来也必须生效。
        merge_to_file(&[second, first], &output).unwrap();
        assert_eq!(merged_widths(&output), vec![200, 100, 110]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
