// Renders every tryout, dark and light, through the real build's post-processing (forOutlook).
// Usage, from emails/: npx tsx prototypes/render.tsx <out dir>
// Writes <id>-<field>.html with local file:// pictures to look at, and sizes.json with each
// file's size as sent (pictures at https://app.oryxa.in/email, as the real build links them).
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { render } from "@react-email/components";
import { forOutlook } from "../src/outlook.ts";
import { type Field, FieldContext } from "../src/theme.ts";
import { type Props, sample, tryouts } from "./tryouts.tsx";

const out = process.argv[2];
if (!out) throw new Error("usage: tsx prototypes/render.tsx <out dir>");
await mkdir(out, { recursive: true });

const here = import.meta.dirname;
const local = {
  assets: pathToFileURL(resolve(here, "../../web/public/email")).href,
  proto: pathToFileURL(resolve(here, "assets")).href,
};
const live = { assets: "https://app.oryxa.in/email", proto: "https://app.oryxa.in/email" };

const html = async (id: keyof typeof tryouts, field: Field, base: typeof local) => {
  const Tryout = tryouts[id].component;
  const props: Props = { ...sample, ...base };
  return forOutlook(
    await render(
      <FieldContext.Provider value={field}>
        <Tryout {...props} />
      </FieldContext.Provider>,
    ),
  );
};

const sizes: Record<string, number> = {};
for (const id of Object.keys(tryouts) as (keyof typeof tryouts)[]) {
  for (const field of ["dark", "light"] as Field[]) {
    await writeFile(`${out}/${id}-${field}.html`, await html(id, field, local));
    sizes[`${id}-${field}`] = Buffer.byteLength(await html(id, field, live));
  }
}
await writeFile(`${out}/sizes.json`, `${JSON.stringify(sizes, null, 2)}\n`);
console.log(sizes);
