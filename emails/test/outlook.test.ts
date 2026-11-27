import assert from "node:assert/strict";
import { test } from "node:test";
import { forOutlook } from "../src/outlook.ts";

const page = (body: string) => `<html dir="ltr" lang="en"><head></head><body>${body}</body></html>`;

test("a square figure becomes a VML rect around its content", () => {
  const html = forOutlook(
    page(
      '<div data-vml-open="https://x/a.jpg" data-vml-rounded="false"></div>code<div data-vml-close="false"></div>',
    ),
  );
  assert.match(
    html,
    /<!--\[if gte mso 9\]><v:rect [^>]*><v:fill type="frame" src="https:\/\/x\/a.jpg"/,
  );
  assert.match(html, /code<!--\[if gte mso 9\]><\/v:textbox><\/v:rect><!\[endif\]-->/);
  assert.doesNotMatch(html, /data-vml/);
});

test("a rounded figure becomes a VML roundrect and closes as one", () => {
  const html = forOutlook(
    page(
      '<div data-vml-open="a.jpg" data-vml-rounded="true"></div>x<div data-vml-close="true"></div>',
    ),
  );
  assert.match(html, /<v:roundrect arcsize="9%"/);
  assert.match(html, /<\/v:roundrect><!\[endif\]-->/);
});

test("React's render markers go and Outlook gets its namespaces and 96 DPI", () => {
  const html = forOutlook(page("<!--$--><p>hi</p><!--/$--><!--html--><!--/head-->"));
  assert.doesNotMatch(html, /<!--\/?\$-->|<!--\/?(html|head|body)-->/);
  assert.match(html, /^<html xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o=/);
  assert.match(html, /<o:PixelsPerInch>96<\/o:PixelsPerInch>.*<\/head>/);
});
