import { Body, Button, Container, Head, Html, Link, Preview, Text } from "@react-email/components";
import type { CSSProperties, ReactNode } from "react";
import { color, font, type Scene } from "./theme.ts";

const SUPPORT = "support@oryxa.in";

/**
 * One picture behind the top of the email, faded into the night page (the fade is baked into the
 * JPG), with everything set straight on it: no panels, no cards. Where a mail app drops
 * background images (Outlook on Windows), the same words sit on plain night.
 */
export function Layout(props: {
  assets: string;
  preview: string;
  scene: Scene;
  /** Who the email went to, shown in the footer. */
  to: string;
  children: ReactNode;
}) {
  const picture = `${props.assets}/${props.scene}.jpg`;
  // The legacy `background` attribute reaches the mail apps that ignore the CSS one.
  const legacy = { background: picture, bgcolor: color.space } as Record<string, string>;
  return (
    <Html lang="en">
      <Head>
        {/* Declared so mail apps keep these colours instead of inverting them in dark mode. */}
        <meta name="color-scheme" content="dark" />
        <meta name="supported-color-schemes" content="dark" />
        <link
          rel="stylesheet"
          href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700&display=swap"
        />
      </Head>
      <Preview>{props.preview}</Preview>
      <Body style={body}>
        <Container style={container}>
          <table width="100%" cellPadding={0} cellSpacing={0} role="presentation" {...legacy}>
            <tbody>
              <tr>
                <td
                  style={{
                    backgroundColor: color.space,
                    backgroundImage: `url(${picture})`,
                    backgroundSize: "100% auto",
                    backgroundPosition: "top center",
                    backgroundRepeat: "no-repeat",
                    padding: "36px 32px 44px",
                  }}
                >
                  <Wordmark />
                  {/* Leaves the sky clear before the words begin where the picture fades. */}
                  <div style={{ height: 196, lineHeight: "196px", fontSize: 1 }}>&nbsp;</div>
                  {props.children}
                  <div style={rule} />
                  <Wordmark small />
                  <Text style={{ ...footer, marginTop: 12 }}>
                    Sent to {props.to}. Questions? Write to{" "}
                    <Link href={`mailto:${SUPPORT}`} style={{ color: color.nightInk2 }}>
                      {SUPPORT}
                    </Link>
                    .
                  </Text>
                  <Text style={{ ...footer, marginTop: 4 }}>Oryxa AI Private Limited</Text>
                </td>
              </tr>
            </tbody>
          </table>
        </Container>
      </Body>
    </Html>
  );
}

function Wordmark({ small = false }: { small?: boolean }) {
  const size = small ? 17 : 22;
  return (
    <Text style={{ ...wordmark, fontSize: size, lineHeight: `${size}px` }}>
      oryxa
      <span style={dot} />
    </Text>
  );
}

export function Headline({ children }: { children: ReactNode }) {
  return <Text style={headline}>{children}</Text>;
}

export function Para({ children }: { children: ReactNode }) {
  return <Text style={para}>{children}</Text>;
}

export function Meta({ children }: { children: ReactNode }) {
  return <Text style={meta}>{children}</Text>;
}

/** A one-time code on a strip of glass, large and spaced for reading off one screen into another. */
export function Code({ value }: { value: string }) {
  return (
    <div style={{ ...glass, margin: "28px 0 0", padding: "20px 24px" }}>
      <Text style={code}>{value}</Text>
    </div>
  );
}

/** The one action, with its link spelled out under it for mail apps that drop buttons. */
export function Action({ href, children }: { href: string; children: ReactNode }) {
  return (
    <>
      <div style={{ margin: "30px 0 0" }}>
        <Button href={href} style={button}>
          {children}&nbsp;&nbsp;→
        </Button>
      </div>
      <Text style={{ ...meta, marginTop: 22 }}>
        Or paste this link into your browser:
        <br />
        <Link href={href} style={{ color: color.periwinkle, wordBreak: "break-all" }}>
          {href}
        </Link>
      </Text>
    </>
  );
}

/** Label and value rows split by faint hairlines, straight on the page. */
export function Ledger({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <table
      width="100%"
      cellPadding={0}
      cellSpacing={0}
      role="presentation"
      style={{ margin: "26px 0 0" }}
    >
      <tbody>
        {rows.map(([label, value]) => (
          <tr key={label}>
            <td style={{ ...ledgerCell, color: color.nightInk3, width: 112 }}>{label}</td>
            <td style={{ ...ledgerCell, color: color.nightInk, fontWeight: 500 }}>{value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** A glass status pill; the gold ember dot appears only for what needs the person. */
export function NeedsYou() {
  return (
    <Text style={pill}>
      <span style={{ ...dot, width: 7, height: 7, borderRadius: 4, margin: "0 7px 1px 0" }} />
      Needs you
    </Text>
  );
}

const body: CSSProperties = {
  margin: 0,
  backgroundColor: color.space,
  fontFamily: font,
  WebkitFontSmoothing: "antialiased",
};
const container: CSSProperties = { maxWidth: 600, padding: "16px 0 32px" };
// White at low alpha over the picture or the night reads as glass; mail apps cannot blur.
const glass: CSSProperties = {
  backgroundColor: "rgba(255,255,255,0.07)",
  border: "1px solid rgba(255,255,255,0.14)",
  borderRadius: 18,
};
const wordmark: CSSProperties = {
  margin: 0,
  fontWeight: 700,
  letterSpacing: "-0.055em",
  color: color.nightInk,
};
const dot: CSSProperties = {
  display: "inline-block",
  width: "0.2em",
  height: "0.2em",
  marginLeft: "0.1em",
  borderRadius: "0.14em",
  backgroundColor: color.ember,
};
const headline: CSSProperties = {
  margin: 0,
  fontSize: 32,
  lineHeight: "36px",
  fontWeight: 600,
  letterSpacing: "-0.025em",
  color: "#ffffff",
};
const para: CSSProperties = {
  margin: "14px 0 0",
  fontSize: 16,
  lineHeight: "25px",
  color: color.nightInk2,
};
const meta: CSSProperties = {
  margin: "24px 0 0",
  fontSize: 14,
  lineHeight: "21px",
  color: color.nightInk3,
};
const code: CSSProperties = {
  margin: 0,
  fontSize: 32,
  lineHeight: "36px",
  fontWeight: 600,
  letterSpacing: "0.3em",
  fontVariantNumeric: "tabular-nums",
  color: "#ffffff",
};
const button: CSSProperties = {
  display: "inline-block",
  backgroundColor: "#ffffff",
  color: color.navy,
  borderRadius: 9999,
  padding: "13px 22px",
  fontSize: 15,
  lineHeight: "15px",
  fontWeight: 600,
  letterSpacing: "-0.01em",
  textDecoration: "none",
};
const ledgerCell: CSSProperties = {
  padding: "12px 0",
  borderTop: "1px solid rgba(255,255,255,0.1)",
  fontSize: 14,
  lineHeight: "20px",
  verticalAlign: "top",
};
const pill: CSSProperties = {
  ...glass,
  display: "inline-block",
  margin: "0 0 18px",
  padding: "5px 12px",
  borderRadius: 9999,
  fontSize: 12,
  lineHeight: "16px",
  fontWeight: 500,
  color: color.nightInk,
};
const rule: CSSProperties = {
  height: 1,
  lineHeight: "1px",
  fontSize: 1,
  margin: "44px 0 28px",
  backgroundColor: "rgba(255,255,255,0.1)",
};
const footer: CSSProperties = {
  margin: 0,
  fontSize: 13,
  lineHeight: "19px",
  color: color.nightInk3,
};
