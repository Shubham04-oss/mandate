import { brand } from "./theme.ts";

const OPEN = /<div data-vml-open="([^"]*)" data-vml-rounded="(true|false)"><\/div>/g;
const CLOSE = /<div data-vml-close="(true|false)"><\/div>/g;

/**
 * Outlook on Windows ignores CSS backgrounds; a VML shape behind the figure gives it the picture
 * too. React cannot write conditional comments, so the figure's markers are swapped here. Also
 * adds the VML namespaces and 96 DPI (so Outlook does not rescale on large displays), and drops
 * React's render markers.
 */
export function forOutlook(html: string): string {
  return html
    .replace(OPEN, (_, src: string, rounded: string) => {
      const shape = rounded === "true" ? 'v:roundrect arcsize="9%"' : "v:rect";
      return `<!--[if gte mso 9]><${shape} xmlns:v="urn:schemas-microsoft-com:vml" fill="true" stroke="false" style="width:536px;mso-fit-shape-to-text:true"><v:fill type="frame" src="${src}" color="${brand.space}"/><v:textbox inset="0,0,0,0"><![endif]-->`;
    })
    .replace(CLOSE, (_, rounded: string) => {
      const tag = rounded === "true" ? "v:roundrect" : "v:rect";
      return `<!--[if gte mso 9]></v:textbox></${tag}><![endif]-->`;
    })
    .replace(/<!--\/?\$-->|<!--\/?(html|head|body)-->/g, "")
    .replace(
      "<html ",
      '<html xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" ',
    )
    .replace(
      "</head>",
      "<!--[if mso]><xml><o:OfficeDocumentSettings><o:AllowPNG/><o:PixelsPerInch>96</o:PixelsPerInch></o:OfficeDocumentSettings></xml><![endif]--></head>",
    );
}
