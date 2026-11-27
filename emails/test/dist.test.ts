// Checks the built emails in dist/ (npm test builds them first).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const manifest: Record<string, { subject: string; placeholders: string[] }> = JSON.parse(
  readFileSync("dist/manifest.json", "utf8"),
);
const read = (f: string) => readFileSync(`dist/${f}`, "utf8");

for (const [name, { placeholders }] of Object.entries(manifest)) {
  for (const file of [`${name}.html`, `${name}.light.html`]) {
    test(`${file} fills every placeholder and is ready for Outlook and Gmail`, () => {
      const html = read(file);
      for (const p of placeholders) assert.ok(html.includes(`{{${p}}}`), `missing {{${p}}}`);
      assert.ok(!/data-vml|<!--\/?\$-->/.test(html), "render markers left behind");
      assert.ok(html.includes('<v:fill type="frame"'), "no VML picture for Outlook");
      // Gmail drops <body> styles: a full-width table carries the page colour as attribute and CSS.
      const page = file.endsWith(".light.html") ? "#f3f4fa" : "#04050f";
      assert.ok(
        new RegExp(
          `<table width="100%"[^>]*bgcolor="${page}" style="background-color:${page}`,
        ).test(html),
        "no page colour on the wrapper table",
      );
      // Gmail clips messages over 102 KB and hides the rest behind a link.
      assert.ok(Buffer.byteLength(html) < 102_000, `${file} is too large for Gmail`);
    });
  }

  test(`${name}.txt carries what the reader needs to act`, () => {
    const text = read(`${name}.txt`);
    if (placeholders.includes("code")) assert.match(text, /^\{\{code\}\}$/m);
    // A link stands alone on its line, so it can be tapped or copied whole.
    if (placeholders.includes("url")) assert.match(text, /:\n\{\{url\}\}\n/);
    assert.doesNotMatch(text, /→|Button not working/);
  });
}

test("a one-time code is selectable in one tap", () => {
  for (const name of ["sign-in-code", "verify-email"]) {
    assert.match(read(`${name}.html`), /user-select:all[^>]*>\{\{code\}\}/);
  }
});
