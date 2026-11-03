// Writes the licence notices that travel with the installer: every Rust crate and npm package that ends up in
// the program, the two models, and ONNX Runtime, each with its licence text (each text once).
//
//   node tools/make-notices.mjs            write app/src-tauri/resources/third-party-notices.txt
//   node tools/make-notices.mjs --check    fail if that file is not what would be written now
//
// Needs `cargo` (for `cargo metadata`, offline once the crates are fetched) and `app/node_modules`
// (`npm ci` in `app`). Fails, naming the package, when a licence text cannot be found, rather than
// leaving a gap.

import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "..");
const OUTPUT = path.join(root, "app", "src-tauri", "resources", "third-party-notices.txt");

const LICENCE_FILE = /^(licen[cs]e|copying|unlicen[cs]e|notice)([-_.].*)?$/i;

/** The text of a file, with line endings made plain, so the same licence in two places counts as one. */
export const tidy = (text) => text.replace(/\r\n?/g, "\n").replace(/[ \t]+$/gm, "").trim();

/** Licence and notice files directly inside a package folder (and one level down in `licenses/`). */
export function licenceFiles(dir) {
  const found = [];
  const look = (folder) => {
    for (const entry of fs.readdirSync(folder, { withFileTypes: true })) {
      if (entry.isFile() && LICENCE_FILE.test(entry.name) && !/\.(rs|js|ts|json|toml|html|png)$/i.test(entry.name)) {
        found.push(path.join(folder, entry.name));
      }
    }
  };
  look(dir);
  for (const sub of ["licenses", "licences", "LICENSES"]) {
    const folder = path.join(dir, sub);
    if (fs.existsSync(folder) && fs.statSync(folder).isDirectory()) look(folder);
  }
  // On Windows `licenses` and `LICENSES` are the same folder: list each file once.
  return [...new Map(found.map((f) => [f.toLowerCase(), f])).values()].sort();
}

/** Group packages by the text of their licence files: `{ text, packages: ["name version", ...] }`, biggest groups first. */
export function groupByText(entries) {
  const groups = new Map();
  for (const entry of entries) {
    for (const text of entry.texts) {
      const key = createHash("sha256").update(text).digest("hex");
      if (!groups.has(key)) groups.set(key, { text, packages: [] });
      const group = groups.get(key);
      const label = `${entry.name} ${entry.version}`;
      if (!group.packages.includes(label)) group.packages.push(label);
    }
  }
  return [...groups.values()].sort((a, b) => b.packages.length - a.packages.length || a.text.localeCompare(b.text));
}

function rustPackages() {
  const meta = JSON.parse(
    execSync("cargo metadata --format-version 1 --locked --filter-platform x86_64-pc-windows-msvc", { cwd: root, maxBuffer: 1 << 28 }).toString(),
  );
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const start = meta.packages.find((p) => p.name === "oxicut-app").id;
  const seen = new Set();
  const stack = [start];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id).deps) if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
  }
  return [...seen]
    .map((id) => byId.get(id))
    .filter((p) => p.source !== null) // OxiCut's own crates are not "third party"
    .map((p) => ({
      kind: "Rust",
      name: p.name,
      version: p.version,
      licence: p.license ?? "unknown",
      home: p.repository || p.homepage || "",
      dir: path.dirname(p.manifest_path),
    }));
}

function npmPackages() {
  const appDir = path.join(root, "app");
  const lock = JSON.parse(fs.readFileSync(path.join(appDir, "package-lock.json"), "utf8"));
  const wanted = Object.keys(JSON.parse(fs.readFileSync(path.join(appDir, "package.json"), "utf8")).dependencies ?? {});
  const seen = new Map();
  const walk = (name) => {
    if (seen.has(name)) return;
    const entry = lock.packages[`node_modules/${name}`];
    if (!entry) return;
    seen.set(name, entry);
    for (const dep of Object.keys(entry.dependencies ?? {})) walk(dep);
  };
  wanted.forEach(walk);
  return [...seen.entries()].map(([name, entry]) => {
    const dir = path.join(appDir, "node_modules", name);
    let licence = entry.license;
    let home = "";
    try {
      const pkg = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
      licence ??= typeof pkg.license === "string" ? pkg.license : pkg.license?.type;
      home = typeof pkg.repository === "string" ? pkg.repository : (pkg.repository?.url ?? pkg.homepage ?? "");
    } catch {
      /* the lock file's entry is all there is */
    }
    return { kind: "npm", name, version: entry.version, licence: licence ?? "unknown", home: home.replace(/^git\+/, "").replace(/\.git$/, ""), dir };
  });
}

const MODELS = [
  {
    title: "MobileSAM (needed by the Careful (v7) detection method; downloaded by the person who chooses it)",
    lines: [
      "Original project: https://github.com/ChaoningZhang/MobileSAM, Apache License 2.0 (text below).",
      "The ONNX files OxiCut downloads are Acly's export: https://huggingface.co/Acly/MobileSAM, published under the MIT licence.",
    ],
    file: "mobilesam-LICENSE.txt",
  },
  {
    title: "Orientation model (needed for turning the pictures automatically; downloaded when switched on)",
    lines: ["deep-image-orientation-detection by Duarte Barbosa: https://github.com/duartebarbosadev/deep-image-orientation-detection, MIT licence (text below)."],
    file: "orientation-model-LICENSE.txt",
  },
  {
    title: "ONNX Runtime (runs the models)",
    lines: [
      "https://github.com/microsoft/onnxruntime, MIT licence (text below). Its own list of third-party notices is at",
      "https://github.com/microsoft/onnxruntime/blob/main/ThirdPartyNotices.txt",
    ],
    file: "onnxruntime-LICENSE.txt",
  },
];

export function build() {
  const packages = [...rustPackages(), ...npmPackages()].sort((a, b) => a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name));
  const missing = [];
  const entries = packages.map((p) => {
    let texts = licenceFiles(p.dir).map((f) => tidy(fs.readFileSync(f, "utf8"))).filter(Boolean);
    // A few packages leave the licence file out of what they publish; the text kept here is their upstream's.
    const upstream = path.join(here, "licenses", "missing", `${p.name}-LICENSE.txt`);
    if (!texts.length && fs.existsSync(upstream)) texts = [tidy(fs.readFileSync(upstream, "utf8"))];
    if (!texts.length) missing.push(`${p.name} ${p.version} (${p.licence}) in ${p.dir}`);
    return { ...p, texts };
  });
  if (missing.length) {
    throw new Error(`No licence text found for ${missing.length} package(s):\n  ${missing.join("\n  ")}`);
  }

  const out = [];
  const rule = "=".repeat(78);
  out.push(
    "OXICUT: LICENCES OF THE PARTS IT IS BUILT FROM",
    "",
    "OxiCut is free software under the GNU General Public License, version 3 (see the LICENSE file",
    "and https://github.com/oxicut/oxicut). It is built from the open-source parts listed here, and uses",
    "the models named below. Their licences require their notices to come with the program; here they are.",
    "",
    "Some parts (the MPL-2.0 ones) are under a file-by-file licence: the source of those parts is public",
    "at the addresses given.",
    "",
    rule,
    "THE MODELS AND THE RUNTIME THAT RUNS THEM",
    rule,
  );
  for (const model of MODELS) {
    out.push("", model.title, ...model.lines.map((l) => `  ${l}`), "", ...tidy(fs.readFileSync(path.join(here, "licenses", model.file), "utf8")).split("\n").map((l) => (l ? `    ${l}` : "")), "");
  }
  out.push(rule, "PACKAGES", rule, "");
  for (const p of packages) out.push(`${p.name} ${p.version} (${p.kind})  ${p.licence}${p.home ? `  ${p.home}` : ""}`);
  out.push("", rule, "LICENCE TEXTS", rule);
  for (const group of groupByText(entries)) {
    out.push("", "-".repeat(78), "Used by:", ...wrap(group.packages.join(", "), 76).map((l) => `  ${l}`), "-".repeat(78), "", group.text, "");
  }
  return out.join("\n").replace(/\n{4,}/g, "\n\n\n") + "\n";
}

function wrap(text, width) {
  const lines = [];
  let line = "";
  for (const word of text.split(" ")) {
    if (line && line.length + word.length + 1 > width) {
      lines.push(line);
      line = word;
    } else line = line ? `${line} ${word}` : word;
  }
  if (line) lines.push(line);
  return lines;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const text = build();
    if (process.argv.includes("--check")) {
      const current = fs.existsSync(OUTPUT) ? fs.readFileSync(OUTPUT, "utf8").replace(/\r\n/g, "\n") : "";
      if (current !== text) {
        console.error("app/src-tauri/resources/third-party-notices.txt is out of date. Run: node tools/make-notices.mjs");
        process.exit(1);
      }
      console.log("The licence notices are up to date.");
    } else {
      fs.mkdirSync(path.dirname(OUTPUT), { recursive: true });
      fs.writeFileSync(OUTPUT, text);
      console.log(`Wrote ${path.relative(root, OUTPUT)} (${Math.round(text.length / 1024)} KB).`);
    }
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
