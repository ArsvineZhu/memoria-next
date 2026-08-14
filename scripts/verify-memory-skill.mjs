import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const skillRoot = join(root, "skills", "memoria");
const required = [
  "Stable references are retrieved, never invented.",
  "Most conversational turns do not create long-term memory.",
  "Returned-but-unused is not negative feedback.",
  "Association evidence is not causal authority.",
  "Do not blindly retry HEAD conflicts.",
];
const references = [
  "references/authoring.md",
  "references/querying.md",
  "references/feedback.md",
  "references/governance.md",
  "references/examples.md",
];

const contents = [
  await readFile(join(skillRoot, "SKILL.md"), "utf8"),
  ...(await Promise.all(
    references.map((reference) => readFile(join(skillRoot, reference), "utf8")),
  )),
].join("\n");
const missing = required.filter((rule) => !contents.includes(rule));

if (missing.length > 0) {
  console.error("Memoria Skill verification failed.");
  for (const rule of missing) {
    console.error("- missing: " + rule);
  }
  process.exitCode = 1;
} else {
  console.log("Memoria Skill verification passed.");
}
