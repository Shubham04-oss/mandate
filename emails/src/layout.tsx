import {
  Body,
  Button,
  Container,
  Head,
  Html,
  Img,
  Link,
  Preview,
  Section,
  Text,
} from "@react-email/components";
import type { CSSProperties, ReactNode } from "react";
import { color, font, type Scene } from "./theme.ts";

const SUPPORT = "support@oryxa.in";

/** The night page, the oryxa wordmark, one moon scene, and a moonlight panel holding the message. */
export function Layout(props: {
  assets: string;
  preview: string;
  scene: Scene;
  /** Who the email went to, shown in the footer. */
  to: string;
  children: ReactNode;
}) {
  return (
    <Html lang="en">
      <Head>
        {/* Declared so mail apps keep these colours instead of inverting them in dark mode. */}
        <meta name="color-scheme" content="light dark" />
        <meta name="supported-color-schemes" content="light dark" />
        <link
          rel="stylesheet"
          href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700&display=swap"
        />
      </Head>
      <Preview>{props.preview}</Preview>
      <Body style={body}>
        <Container style={container}>
          <Section style={{ padding: "8px 8px 24px" }}>
            <Wordmark />
          </Section>
          <Img
            src={`${props.assets}/${props.scene}.jpg`}
            width="568"
            alt=""
            style={{ display: "block", width: "100%", height: "auto", borderRadius: 24 }}
          />
          <Section style={panel}>{props.children}</Section>
          <Section style={{ padding: "28px 8px 8px" }}>
            <Text style={footer}>
              Sent by oryxa to {props.to}. Questions? Write to{" "}
              <Link
                href={`mailto:${SUPPORT}`}
                style={{ color: color.nightInk, textDecoration: "underline" }}
              >
                {SUPPORT}
              </Link>
              .
            </Text>
            <Text style={{ ...footer, marginTop: 8 }}>Oryxa AI Private Limited</Text>
          </Section>
        </Container>
      </Body>
    </Html>
  );
}

function Wordmark() {
  return (
    <Text style={wordmark}>
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

/** A one-time code, large and spaced for reading off one screen into another. */
export function Code({ value }: { value: string }) {
  return (
    <Section style={{ ...card, margin: "28px 0 0", padding: "22px 16px", textAlign: "center" }}>
      <Text style={code}>{value}</Text>
    </Section>
  );
}

/** The one action, with its link spelled out under it for mail apps that drop buttons. */
export function Action({ href, children }: { href: string; children: ReactNode }) {
  return (
    <>
      <Section style={{ margin: "28px 0 0" }}>
        <Button href={href} style={button}>
          {children}&nbsp;&nbsp;→
        </Button>
      </Section>
      <Text style={{ ...meta, marginTop: 20 }}>
        Or paste this link into your browser:
        <br />
        <Link href={href} style={{ color: color.sea, wordBreak: "break-all" }}>
          {href}
        </Link>
      </Text>
    </>
  );
}

/** Label and value rows on a white card, split by hairlines. */
export function Ledger({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <Section style={{ ...card, margin: "28px 0 0", padding: "4px 20px" }}>
      {rows.map(([label, value], i) => (
        <table
          key={label}
          width="100%"
          cellPadding={0}
          cellSpacing={0}
          role="presentation"
          style={{ borderTop: i === 0 ? "none" : `1px solid ${color.line}` }}
        >
          <tbody>
            <tr>
              <td style={{ ...ledgerCell, color: color.ink3, width: 112 }}>{label}</td>
              <td style={{ ...ledgerCell, color: color.navy, fontWeight: 500 }}>{value}</td>
            </tr>
          </tbody>
        </table>
      ))}
    </Section>
  );
}

/** The status pill; the gold ember dot appears only for what needs the person. */
export function NeedsYou() {
  return (
    <Text style={pill}>
      <span style={{ ...dot, width: 7, height: 7, borderRadius: 4, margin: "0 6px 1px 0" }} />
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
const container: CSSProperties = { maxWidth: 600, padding: "32px 16px 40px" };
const panel: CSSProperties = {
  marginTop: 12,
  backgroundColor: color.moonlight,
  borderRadius: 28,
  padding: "36px 32px 32px",
};
const card: CSSProperties = {
  backgroundColor: color.card,
  borderRadius: 20,
  border: `1px solid ${color.line}`,
};
const wordmark: CSSProperties = {
  margin: 0,
  fontSize: 26,
  lineHeight: "26px",
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
  fontSize: 28,
  lineHeight: "32px",
  fontWeight: 600,
  letterSpacing: "-0.02em",
  color: color.navy,
};
const para: CSSProperties = {
  margin: "14px 0 0",
  fontSize: 16,
  lineHeight: "25px",
  color: color.ink2,
};
const meta: CSSProperties = {
  margin: "24px 0 0",
  fontSize: 14,
  lineHeight: "20px",
  color: color.ink3,
};
const code: CSSProperties = {
  margin: 0,
  fontSize: 36,
  lineHeight: "40px",
  fontWeight: 600,
  letterSpacing: "0.24em",
  // Keeps the last digit centred despite the trailing letter-spacing.
  paddingLeft: "0.24em",
  fontVariantNumeric: "tabular-nums",
  color: color.navy,
};
const button: CSSProperties = {
  display: "inline-block",
  backgroundColor: color.navy,
  color: color.moonlight,
  borderRadius: 9999,
  padding: "13px 22px",
  fontSize: 15,
  lineHeight: "15px",
  fontWeight: 600,
  letterSpacing: "-0.01em",
  textDecoration: "none",
};
const ledgerCell: CSSProperties = {
  padding: "13px 0",
  fontSize: 14,
  lineHeight: "20px",
  verticalAlign: "top",
};
const pill: CSSProperties = {
  display: "inline-block",
  margin: "0 0 16px",
  padding: "4px 10px",
  borderRadius: 9999,
  backgroundColor: color.subtle,
  fontSize: 12,
  lineHeight: "16px",
  fontWeight: 500,
  color: color.ink2,
};
const footer: CSSProperties = {
  margin: 0,
  fontSize: 13,
  lineHeight: "19px",
  color: color.nightInk3,
};
