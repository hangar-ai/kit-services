"use strict";
// Hangar's own files, which a test compares against: the docs, so that a method the kit relies on
// is documented. Hangar ships them, so this repository holds no copy. CI gets them at the tag the
// manifest's `hangar` names and passes the folder in HANGAR_UI_DIR. Without them, the test that
// needs them skips and says so.
//
//   node test/hangar-ui.js --get           into .hangar-ui/, which git ignores
//   node test/hangar-ui.js --get v2.0.0    from another Hangar tag
const fs = require("node:fs");
const path = require("node:path");

const KIT = path.join(__dirname, "..");
const DIR = process.env.HANGAR_UI_DIR ? path.resolve(process.env.HANGAR_UI_DIR) : path.join(KIT, ".hangar-ui");
const FILES = {
  "kits.md": "docs/kits.md",
};
const HANGAR_FILES_MISSING = "Hangar's files are not here: run `node test/hangar-ui.js --get`, then the tests again";

/** The path of one of Hangar's files, or null when it is not here. */
function hangarFile(name) {
  const file = path.join(DIR, name);
  return fs.existsSync(file) ? file : null;
}

async function get(ref) {
  const manifest = JSON.parse(fs.readFileSync(path.join(KIT, "hangar-kit.json"), "utf8"));
  const tag = ref || `v${String(manifest.hangar).match(/\d+\.\d+\.\d+/)}`;
  fs.mkdirSync(DIR, { recursive: true });
  for (const [name, source] of Object.entries(FILES)) {
    const answer = await fetch(`https://raw.githubusercontent.com/hangar-ai/hangar/${tag}/${source}`);
    if (!answer.ok) throw new Error(`Hangar ${tag} has no ${source} (${answer.status}). It may not be tagged yet.`);
    fs.writeFileSync(path.join(DIR, name), Buffer.from(await answer.arrayBuffer()));
  }
  console.log(`Hangar's files of ${tag} are in ${DIR}`);
}

if (process.argv[2] === "--get") {
  get(process.argv[3]).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}

module.exports = { hangarFile, HANGAR_FILES_MISSING };
