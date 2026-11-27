// Prototype: the sign-in code email with no picture, built only from the Oryxa language's own
// parts (split header, the dusk-violet blend, glass, a nebula hairline). Not part of the build.
// Run from emails/: npx tsx prototypes/no-picture.tsx
import { mkdir, writeFile } from "node:fs/promises";
import { Body, Container, Head, Html, Preview, render, Text } from "@react-email/components";
import type { CSSProperties } from "react";
import { brand, font } from "../src/theme.ts";

const dusk = { top: "#17142a", mid: "#2f2b52", foot: "#3f3864" };
const nebula = { amber: "#bf795c", rose: "#8a668b", violet: "#6c5682" };
const ink = { 1: "#eef0fa", 2: "#aeb4d0", 3: "#8f96b8" };

function Wordmark({ size }: { size: number }) {
  return (
    <Text
      style={{
        margin: 0,
        fontSize: size,
        lineHeight: `${size}px`,
        fontWeight: 700,
        letterSpacing: "-0.055em",
        color: ink[1],
      }}
    >
      oryxa
      <span style={dot} />
    </Text>
  );
}

function SignInCode(p: { email: string; code: string; expiresIn: string }) {
  // The blend's fallback colour sits on the rounded cell, for apps that drop gradients.
  const legacy = { bgcolor: dusk.mid } as Record<string, string>;
  return (
    <Html lang="en">
      <Head>
        <meta name="color-scheme" content="dark" />
        <meta name="supported-color-schemes" content="dark" />
        <link
          rel="stylesheet"
          href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700&display=swap"
        />
      </Head>
      <Preview>Enter it on the sign-in page to continue.</Preview>
      <Body style={{ margin: 0, backgroundColor: brand.space, fontFamily: font }}>
        <Container style={{ maxWidth: 600, padding: "52px 32px 40px" }}>
          <Wordmark size={22} />

          {/* Split header, stacked for email: one idea, then one sentence. */}
          <Text style={headline}>Your sign-in code is ready.</Text>
          <Text style={sub}>
            Enter it on the sign-in page to continue as{" "}
            <span style={{ color: ink[1] }}>{p.email}</span>.
          </Text>

          {/* The figure: the dusk-violet blend where a picture would be, the code on glass. */}
          <table
            width="100%"
            cellPadding={0}
            cellSpacing={0}
            role="presentation"
            style={{ marginTop: 40 }}
          >
            <tbody>
              <tr>
                <td {...legacy} style={band}>
                  <Text style={label}>Sign-in code</Text>
                  <div style={glass}>
                    <Text style={code}>{p.code}</Text>
                  </div>
                  <Text style={{ ...label, margin: "16px 0 0" }}>
                    <span
                      style={{
                        ...dot,
                        // The waiting dot: the ember stays for the wordmark and Needs you only.
                        backgroundColor: "#a3b2f5",
                        width: 7,
                        height: 7,
                        borderRadius: 4,
                        margin: "0 8px 1px 0",
                      }}
                    />
                    Expires in {p.expiresIn}
                  </Text>
                </td>
              </tr>
            </tbody>
          </table>

          {/* The caption under the figure. */}
          <Text style={caption}>Didn't try to sign in?</Text>
          <Text style={{ ...sub, marginTop: 6 }}>
            You can ignore this email. Nobody gets in without the code.
          </Text>

          <div style={nebulaLine} />
          <Wordmark size={17} />
          <Text style={{ ...foot, marginTop: 12 }}>
            Sent to {p.email}. Questions? Write to{" "}
            <span style={{ color: ink[2] }}>support@oryxa.in</span>.
          </Text>
          <Text style={{ ...foot, marginTop: 4 }}>Oryxa AI Private Limited</Text>
        </Container>
      </Body>
    </Html>
  );
}

const dot: CSSProperties = {
  display: "inline-block",
  width: "0.2em",
  height: "0.2em",
  marginLeft: "0.1em",
  borderRadius: "0.14em",
  backgroundColor: brand.ember,
};
const headline: CSSProperties = {
  margin: "56px 0 0",
  fontSize: 40,
  lineHeight: "42px",
  fontWeight: 600,
  letterSpacing: "-0.03em",
  color: "#ffffff",
};
const sub: CSSProperties = {
  margin: "18px 0 0",
  fontSize: 17,
  lineHeight: "27px",
  color: ink[2],
};
const band: CSSProperties = {
  backgroundColor: dusk.mid,
  backgroundImage: `linear-gradient(180deg, ${dusk.top} 0%, ${dusk.mid} 55%, ${dusk.foot} 100%)`,
  borderRadius: 28,
  padding: "28px 28px 26px",
};
const label: CSSProperties = {
  margin: "0 0 14px",
  fontSize: 13,
  lineHeight: "18px",
  fontWeight: 500,
  color: "rgba(255,255,255,0.7)",
};
const glass: CSSProperties = {
  backgroundColor: "rgba(255,255,255,0.08)",
  border: "1px solid rgba(255,255,255,0.18)",
  borderRadius: 18,
  padding: "22px 24px",
};
const code: CSSProperties = {
  margin: 0,
  fontSize: 40,
  lineHeight: "44px",
  fontWeight: 600,
  letterSpacing: "0.32em",
  fontVariantNumeric: "tabular-nums",
  color: "#ffffff",
};
const caption: CSSProperties = {
  margin: "36px 0 0",
  fontSize: 20,
  lineHeight: "26px",
  fontWeight: 600,
  letterSpacing: "-0.015em",
  color: ink[1],
};
// The nebula's gas colours together, never one alone, as a drawn accent where a picture would sit.
const nebulaLine: CSSProperties = {
  height: 2,
  lineHeight: "2px",
  fontSize: 1,
  margin: "48px 0 28px",
  backgroundColor: nebula.rose,
  backgroundImage: `linear-gradient(90deg, rgba(4,5,15,0) 0%, ${nebula.amber} 25%, ${nebula.rose} 55%, ${nebula.violet} 80%, rgba(4,5,15,0) 100%)`,
};
const foot: CSSProperties = { margin: 0, fontSize: 13, lineHeight: "19px", color: ink[3] };

await mkdir("preview", { recursive: true });
await writeFile(
  "preview/prototype-no-picture.html",
  await render(<SignInCode email="priya@acme.com" code="482913" expiresIn="10 minutes" />),
);
