//! 构建脚本：把图标与版本信息写入 Windows 资源段。
//!
//! 图标由 tools/make-icon.ps1 从 assets/icon-source.png 生成。

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/icon.ico");
    resource.set("ProductName", "PDF 合并工具");
    resource.set("FileDescription", "PDF 合并工具");
    resource.set("LegalCopyright", "MIT License");
    resource
        .compile()
        .expect("嵌入 Windows 资源失败：请确认已安装 Visual Studio C++ 工具链");
}
