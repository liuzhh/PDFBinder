// Bumps the release version in every manifest and creates the matching tag.
//
//   node scripts/bump-version.mjs 0.2.0
//
// Why the bump is committed before the tag is created: the release workflow
// builds whatever the pushed tag points at. Tag first and the tag would still
// resolve to the previous commit, so the release would silently ship the old
// version (or collide with an existing release).
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const CARGO_TOML = "Cargo.toml";
const CARGO_LOCK = "Cargo.lock";

const read = (rel) => fs.readFileSync(path.join(root, rel), "utf8");
const write = (rel, text) => fs.writeFileSync(path.join(root, rel), text);

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function git(...args) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
}

/** Run git, returning null instead of throwing when the command fails. */
function gitOrNull(...args) {
  try {
    return git(...args);
  } catch {
    return null;
  }
}

/** Parse `x.y.z` with an optional `-prerelease`, or return null. */
function parseVersion(value) {
  const m = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(String(value).trim());
  if (!m) return null;
  return { major: +m[1], minor: +m[2], patch: +m[3], pre: m[4] || "" };
}

function compareVersions(a, b) {
  for (const key of ["major", "minor", "patch"]) {
    if (a[key] !== b[key]) return a[key] - b[key];
  }
  if (a.pre === b.pre) return 0;
  if (a.pre === "") return 1;
  if (b.pre === "") return -1;
  return a.pre < b.pre ? -1 : 1;
}

/** Return the `[package]` section of Cargo.toml as a string. */
function packageSection(text) {
  const header = "[package]";
  const start = text.indexOf(header);
  if (start < 0) fail(`${CARGO_TOML}: no [package] section found`);
  const bodyStart = start + header.length;
  const nextSection = text.slice(bodyStart).search(/^\[/m);
  const bodyEnd = nextSection < 0 ? text.length : bodyStart + nextSection;
  return { section: text.slice(0, bodyEnd), bodyEnd };
}

/**
 * Replace `version` inside the `[package]` section only, so entries in
 * `[dependencies]` are left untouched.
 */
function bumpCargoToml(text, next) {
  const { section, bodyEnd } = packageSection(text);
  if (!/^version\s*=\s*"/m.test(section)) fail(`${CARGO_TOML}: no version key in [package]`);
  const bumped = section.replace(/^(version\s*=\s*")([^"]*)(")/m, (_m, prefix, _current, suffix) => prefix + next + suffix);
  return bumped + text.slice(bodyEnd);
}

/** Read the locked version of one crate, or return null. */
function lockedVersion(text, crateName) {
  const pattern = new RegExp(`name = "${crateName}"\\r?\\nversion = "([^"]*)"`);
  return pattern.exec(text)?.[1] ?? null;
}

/** Keep the lockfile in step without shelling out to cargo. */
function bumpCargoLock(text, crateName, next) {
  if (lockedVersion(text, crateName) === null) fail(`${CARGO_LOCK}: no entry for "${crateName}"`);
  const pattern = new RegExp(`(name = "${crateName}"\\r?\\nversion = ")([^"]*)(")`);
  return text.replace(pattern, (_m, prefix, _current, suffix) => prefix + next + suffix);
}

/* ------------------------------------------------------------------ */

const next = process.argv[2];
if (!next) fail("usage: node scripts/bump-version.mjs <x.y.z>");

const nextParsed = parseVersion(next);
if (!nextParsed) fail(`"${next}" is not a valid version; expected something like 0.2.0 or 0.2.0-beta.1`);

const cargoToml = read(CARGO_TOML);
const current = /^version\s*=\s*"([^"]*)"/m.exec(packageSection(cargoToml).section)?.[1];
if (!current) fail(`${CARGO_TOML}: no version key in [package]`);

const currentParsed = parseVersion(current);
if (currentParsed && compareVersions(nextParsed, currentParsed) <= 0) {
  fail(`new version ${next} must be greater than the current ${current}`);
}

// Untracked files are harmless (the bump commit only stages the manifests),
// but an uncommitted edit to a tracked file would end up outside the tagged
// commit, so the release would silently not contain it.
const status = git("status", "--porcelain", "--untracked-files=no");
if (status) {
  fail("you have uncommitted changes to tracked files; commit or stash them first\n" + status);
}

const tag = `v${next}`;
if (gitOrNull("rev-parse", "--verify", "--quiet", `refs/tags/${tag}`)) {
  fail(`tag ${tag} already exists`);
}

const crateName = /^name\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1] ?? "pdfbinder";

write(CARGO_TOML, bumpCargoToml(cargoToml, next));
write(CARGO_LOCK, bumpCargoLock(read(CARGO_LOCK), crateName, next));

// Guard against a silent regex miss on either file.
if (/^version\s*=\s*"([^"]*)"/m.exec(packageSection(read(CARGO_TOML)).section)?.[1] !== next) {
  fail(`${CARGO_TOML}: expected version ${next}`);
}
if (lockedVersion(read(CARGO_LOCK), crateName) !== next) fail(`${CARGO_LOCK}: expected version ${next}`);

git("add", CARGO_TOML, CARGO_LOCK);
git("commit", "-m", `release: v${next}`);
git("tag", tag);

console.log(`bumped ${current} -> ${next} in:`);
for (const rel of [CARGO_TOML, CARGO_LOCK]) console.log(`  ${rel}`);
console.log(`\ncommitted and tagged ${tag}. Next:\n  git push origin main ${tag}`);