import assert from "node:assert/strict";
import { test } from "node:test";
import { tidyText } from "../src/text.ts";

test("a link moves to its own line under its words", () => {
  assert.equal(
    tidyText("Choose a new password : https://app.oryxa.in/reset?token=x"),
    "Choose a new password:\nhttps://app.oryxa.in/reset?token=x\n",
  );
});

test("a colon inside a sentence is left alone", () => {
  assert.equal(tidyText("Note : this stays as written."), "Note : this stays as written.\n");
});

test("runs of blank lines shrink to one and the ends are trimmed", () => {
  assert.equal(tidyText("\n\na\n\n\n\nb\n\n"), "a\n\nb\n");
});
