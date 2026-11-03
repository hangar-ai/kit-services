"use strict";
// Shared by the tests: the recorded output of the system's tools.
const fs = require("node:fs");
const path = require("node:path");

const KIT = path.join(__dirname, "..");
const fixture = (name) => fs.readFileSync(path.join(__dirname, "fixtures", name), "utf8");

module.exports = { KIT, fixture };
