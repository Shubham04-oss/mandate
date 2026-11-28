// Five layout tryouts for the approval email (ORY-92), each in dark and light, for the owner to
// pick from. Not built or sent: prototypes/render.tsx renders them for screenshots.
// ponytail: data-measure spans are for the contrast script only; drop them in the real template.
import { Body, Container, Head, Html, Link, Preview, Text } from "@react-email/components";
import type { CSSProperties, ReactNode } from "react";
import { Action, Ember, Ledger } from "../src/layout.tsx";
import { brand, font, nebula, useField } from "../src/theme.ts";

export type Props = {
  /** web/public/email, where the real pictures live. */
  assets: string;
  /** prototypes/assets, the re-cut pictures these tryouts add. */
  proto: string;
  email: string;
  requester: string;
  agent: string;
  organization: string;
  url: string;
  expiresIn: string;
};

export const sample = {
  email: "raya@acme.com",
  requester: "Priya Shah",
  agent: "Research agent",
  organization: "Acme",
  url: "https://app.oryxa.in/approvals",
  expiresIn: "10 minutes",
};

const SUPPORT = "support@oryxa.in";
const attrs = (a: Record<string, string>) => a as Record<string, string>;

/** Header, the tryout's own middle, the caption and the footer: everything but the middle is shared. */
function Shell({ p, children }: { p: Props; children: ReactNode }) {
  const f = useField();
  return (
    <Html lang="en">
      <Head>
        <title>{`${p.agent} is waiting for your yes`}</title>
        <meta name="format-detection" content="telephone=no,date=no,address=no,email=no,url=no" />
        <meta name="color-scheme" content={f.scheme} />
        <meta name="supported-color-schemes" content={f.scheme} />
        <link
          rel="stylesheet"
          href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400..700&display=swap"
        />
        <style>
          {"@media (max-width:480px){.gap-dark{height:20px!important;line-height:20px!important}" +
            "}@media (min-width:481px){.gap-light{height:330px!important;line-height:330px!important}}" +
            "a[x-apple-data-detectors]{color:inherit!important;text-decoration:none!important}"}
        </style>
      </Head>
      <Preview>{`${p.requester} asked ${p.agent} to do something that needs your approval.`}</Preview>
      <Body style={{ margin: 0, backgroundColor: f.page, fontFamily: font }}>
        <table
          width="100%"
          cellPadding={0}
          cellSpacing={0}
          role="presentation"
          {...attrs({ bgcolor: f.page })}
          style={{ backgroundColor: f.page, fontFamily: font }}
        >
          <tbody>
            <tr>
              <td>
                <Container style={{ maxWidth: 600, padding: "52px 32px 40px" }}>
                  <Wordmark size={22} color={f.strong} />
                  <Text role="heading" aria-level={1} style={{ ...headline, color: f.headline }}>
                    {`${p.agent} is waiting for your yes.`}
                  </Text>
                  <Text style={{ ...sub, color: f.ink2 }}>
                    {`${p.requester} asked ${p.agent} to do something that needs your approval. Nothing happens until you decide.`}
                  </Text>
                  {children}
                  <Text style={{ ...caption, color: f.strong }}>Why you're getting this</Text>
                  <Text style={{ ...sub, marginTop: 6, color: f.ink2 }}>
                    {`You approve requests for ${p.agent} in ${p.organization}.`}
                  </Text>
                  <div style={{ ...nebulaLine, margin: "48px 0 28px" }} />
                  <Wordmark size={17} color={f.strong} />
                  <Text style={{ ...foot, marginTop: 12, color: f.ink3 }}>
                    Sent to {p.email}. Questions? Write to{" "}
                    <Link href={`mailto:${SUPPORT}`} style={{ color: f.ink2 }}>
                      {SUPPORT}
                    </Link>
                    .
                  </Text>
                  <Text style={{ ...foot, marginTop: 4, color: f.ink3 }}>
                    Oryxa AI Private Limited
                  </Text>
                </Container>
              </td>
            </tr>
          </tbody>
        </table>
      </Body>
    </Html>
  );
}

function Wordmark({ size, color }: { size: number; color: string }) {
  return (
    <Text
      style={{
        margin: 0,
        fontSize: size,
        lineHeight: `${size}px`,
        fontWeight: 700,
        letterSpacing: "-0.055em",
        color,
      }}
    >
      oryxa
      <span style={dot} />
    </Text>
  );
}

function Spacer({ px, className }: { px: number; className?: string }) {
  return (
    <div className={className} style={{ height: px, lineHeight: `${px}px`, fontSize: 1 }}>
      &nbsp;
    </div>
  );
}

const rows = (p: Props): [string, ReactNode][] => [
  ["Asked by", p.requester],
  ["Agent", p.agent],
  ["Organization", p.organization],
];

/** The "Needs you" chip, its text in a span so the contrast script can find it. */
function Chip({ style }: { style: CSSProperties }) {
  return (
    <Text style={{ ...chipBase, ...style }}>
      <Ember />
      <span data-measure="chip">Needs you</span>
    </Text>
  );
}

/** "<Agent> wants to", the strong line, and when it expires, in the given colours. */
function Note(props: {
  p: Props;
  label: string;
  title: string;
  expiry: string;
  titleSize?: number;
}) {
  const size = props.titleSize ?? 21;
  return (
    <>
      <Text style={{ margin: 0, fontSize: 13, lineHeight: "18px", color: props.label }}>
        <span data-measure="label">{`${props.p.agent} wants to`}</span>
      </Text>
      <Text
        style={{
          margin: "4px 0 0",
          fontSize: size,
          lineHeight: `${size + 6}px`,
          fontWeight: 600,
          letterSpacing: "-0.015em",
          color: props.title,
          wordBreak: "break-word",
        }}
      >
        <span data-measure="title">Take an action</span>
      </Text>
      <Text
        style={{
          margin: "10px 0 0",
          fontSize: 13,
          lineHeight: "18px",
          fontWeight: 500,
          color: props.expiry,
        }}
      >
        <span style={waiting} />
        <span data-measure="expiry">{`Expires in ${props.p.expiresIn}`}</span>
      </Text>
    </>
  );
}

/** A decorative picture as an <img>: Gmail keeps an img's radius, unlike a td background's. */
function Picture(props: { src: string; radius: string | number; marginTop: number }) {
  return (
    <img
      src={props.src}
      alt=""
      width={536}
      style={{
        display: "block",
        width: "100%",
        maxWidth: "100%",
        height: "auto",
        border: 0,
        borderRadius: props.radius,
        marginTop: props.marginTop,
      }}
    />
  );
}

/** Colours for a solid card on the page: light card on light, navy card on dark. */
function useCard() {
  const f = useField();
  return f.name === "dark"
    ? {
        box: { backgroundColor: brand.navy, border: "1px solid rgba(255,255,255,0.1)" },
        chip: { backgroundColor: brand.space, border: "1px solid rgba(255,255,255,0.12)" },
        chipText: f.strong,
        label: f.ink2,
        title: "#ffffff",
        expiry: f.ink2,
      }
    : {
        box: { backgroundColor: "#ffffff", border: "1px solid rgba(21,27,54,0.1)" },
        chip: { backgroundColor: brand.moonlight, border: "1px solid rgba(21,27,54,0.12)" },
        chipText: brand.navy,
        label: f.ink2,
        title: brand.navy,
        expiry: f.ink3,
      };
}

/** A chip straight on the page colour. */
function usePageChip(): CSSProperties {
  const f = useField();
  return f.name === "dark"
    ? { backgroundColor: brand.navy, border: "1px solid rgba(255,255,255,0.12)", color: f.strong }
    : { backgroundColor: "#ffffff", border: "1px solid rgba(21,27,54,0.12)", color: brand.navy };
}

// A. The current figure, fixed: a picture tall enough to cover the glass at any width, a solid
// fallback sampled from it, and glass strong enough to read over pure white.
export function TryoutA(p: Props) {
  const f = useField();
  const dark = f.name === "dark";
  const picture = dark ? `${p.assets}/moon-dawn-v2-night.jpg` : `${p.proto}/a-light-tall.jpg`;
  // Night fades into the page on every side; the light crop's foot fades into its sky, #0e0b15.
  const fallback = dark ? brand.space : "#0e0b15";
  return (
    <Shell p={p}>
      <table
        width="100%"
        cellPadding={0}
        cellSpacing={0}
        role="presentation"
        style={{ marginTop: 36 }}
      >
        <tbody>
          <tr>
            <td
              {...attrs({ background: picture, bgcolor: fallback })}
              style={{
                backgroundColor: fallback,
                backgroundImage: `url(${picture})`,
                backgroundRepeat: "no-repeat",
                backgroundSize: "100% auto",
                backgroundPosition: "top center",
                ...(dark ? { padding: "34px 40px 30px" } : { padding: 22, borderRadius: 24 }),
              }}
            >
              <div data-vml-open={picture} data-vml-rounded={String(!dark)} />
              <Chip style={{ ...glassA, color: "#ffffff" }} />
              {dark ? <Spacer px={96} className="gap-dark" /> : <Spacer px={12} />}
              <div style={{ ...glassA, borderRadius: 18, padding: "20px 24px" }}>
                <Note p={p} label="#ffffff" title="#ffffff" expiry="#ffffff" />
              </div>
              {/* Light: the glass rides the sky and the dawn shows under it. The 1:1 picture is
              326 px tall at a phone's width, which this fills; only a desktop (with styles) is
              told to grow to 536, so a client that drops <style> crops the picture, never
              overflows it. */}
              {dark ? null : <Spacer px={118} className="gap-light" />}
              <div data-vml-close={String(!dark)} />
            </td>
          </tr>
        </tbody>
      </table>
      <Ledger rows={rows(p)} />
      <Action href={p.url}>Review the request</Action>
    </Shell>
  );
}

// B. The picture as a rounded banner, then the chip and a solid card on the page. No text on it.
export function TryoutB(p: Props) {
  const f = useField();
  const c = useCard();
  const scene = f.name === "dark" ? "night" : "light";
  return (
    <Shell p={p}>
      <Picture src={`${p.assets}/moon-dawn-v2-${scene}.jpg`} radius={20} marginTop={36} />
      <Chip style={{ ...usePageChip(), margin: "24px 0 0" }} />
      <div style={{ ...c.box, borderRadius: 18, padding: "20px 24px", marginTop: 12 }}>
        <Note p={p} label={c.label} title={c.title} expiry={c.expiry} />
      </div>
      <Ledger rows={rows(p)} />
      <Action href={p.url}>Review the request</Action>
    </Shell>
  );
}

// C. The action card leads, with the button in it; a short picture strip closes the details.
export function TryoutC(p: Props) {
  const f = useField();
  const c = useCard();
  const scene = f.name === "dark" ? "night" : "light";
  return (
    <Shell p={p}>
      <div style={{ ...c.box, borderRadius: 20, padding: "24px 24px 26px", marginTop: 36 }}>
        <Chip style={{ ...c.chip, color: c.chipText, margin: "0 0 18px" }} />
        <Note p={p} label={c.label} title={c.title} expiry={c.expiry} titleSize={28} />
        <Action href={p.url}>Review the request</Action>
      </div>
      <Ledger rows={rows(p)} />
      <Picture src={`${p.proto}/c-strip-${scene}.jpg`} radius={16} marginTop={36} />
    </Shell>
  );
}

// D. No picture: a nebula hairline and the ember dot carry the brand.
export function TryoutD(p: Props) {
  const f = useField();
  return (
    <Shell p={p}>
      <div style={{ ...nebulaLine, margin: "36px 0 28px" }} />
      <Text style={{ margin: "0 0 14px", fontSize: 13, lineHeight: "18px", fontWeight: 600 }}>
        <Ember />
        <span data-measure="chip" style={{ color: f.strong }}>
          Needs you
        </span>
      </Text>
      <Note p={p} label={f.ink2} title={f.headline} expiry={f.ink2} titleSize={28} />
      <Ledger rows={rows(p)} />
      <Action href={p.url}>Review the request</Action>
    </Shell>
  );
}

// E. The picture fades into the page colour at its foot (baked into the JPG); text follows on the page.
export function TryoutE(p: Props) {
  const f = useField();
  const dark = f.name === "dark";
  return (
    <Shell p={p}>
      <Picture
        src={`${p.proto}/e-fade-${dark ? "night" : "light"}.jpg`}
        radius={dark ? 0 : "20px 20px 0 0"}
        marginTop={36}
      />
      <Chip style={{ ...usePageChip(), margin: "4px 0 16px" }} />
      <Note p={p} label={f.ink2} title={f.headline} expiry={f.ink2} titleSize={24} />
      <Ledger rows={rows(p)} />
      <Action href={p.url}>Review the request</Action>
    </Shell>
  );
}

export const tryouts = {
  a: { title: "A. Current look, fixed", component: TryoutA },
  b: { title: "B. Picture as a banner", component: TryoutB },
  c: { title: "C. Card first, small picture", component: TryoutC },
  d: { title: "D. No picture", component: TryoutD },
  e: { title: "E. Picture with a fade", component: TryoutE },
} as const;

const dot: CSSProperties = {
  display: "inline-block",
  width: "0.2em",
  height: "0.2em",
  marginLeft: "0.1em",
  borderRadius: "0.14em",
  backgroundColor: brand.ember,
};
const waiting: CSSProperties = {
  display: "inline-block",
  width: 7,
  height: 7,
  borderRadius: 4,
  margin: "0 8px 1px 0",
  backgroundColor: brand.periwinkle,
};
const headline: CSSProperties = {
  margin: "56px 0 0",
  fontSize: 40,
  lineHeight: "42px",
  fontWeight: 600,
  letterSpacing: "-0.03em",
};
const sub: CSSProperties = { margin: "18px 0 0", fontSize: 17, lineHeight: "27px" };
const caption: CSSProperties = {
  margin: "40px 0 0",
  fontSize: 20,
  lineHeight: "26px",
  fontWeight: 600,
  letterSpacing: "-0.015em",
};
const nebulaLine: CSSProperties = {
  height: 2,
  lineHeight: "2px",
  fontSize: 1,
  backgroundColor: nebula.rose,
  backgroundImage: `linear-gradient(90deg, ${nebula.amber} 0%, ${nebula.rose} 50%, ${nebula.violet} 100%)`,
};
const foot: CSSProperties = { margin: 0, fontSize: 13, lineHeight: "19px" };
// Navy glass at 0.72 keeps white text above 6.5:1 even over a pure white pixel.
const glassA: CSSProperties = {
  backgroundColor: "rgba(21,27,54,0.72)",
  border: "1px solid rgba(255,255,255,0.22)",
};
const chipBase: CSSProperties = {
  display: "inline-block",
  margin: 0,
  padding: "5px 12px",
  borderRadius: 9999,
  fontSize: 12,
  lineHeight: "16px",
  fontWeight: 600,
};
