import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

test("MDX syntax foundation is markdown-rs without the retired lexer", () => {
  const manifest = readFileSync(
    join(process.cwd(), "crates", "memoria-mdx", "Cargo.toml"),
    "utf8",
  );

  assert.equal(
    existsSync(
      join(process.cwd(), "crates", "memoria-mdx", "src", "semantic_lexer.rs"),
    ),
    false,
  );
  assert.match(manifest, /markdown\s*=\s*"=1\.0\.0"/);
  assert.doesNotMatch(manifest, /pulldown-cmark/);
});
