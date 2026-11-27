// Prototypes: the sign-in code email in the no-picture layout (split header, one figure, a
// caption, a nebula hairline), with the picture as the figure. One dark, one light.
// Not part of the build. Run from emails/: npx tsx prototypes/with-picture.tsx
import { mkdir, writeFile } from "node:fs/promises";
import { Body, Container, Head, Html, Preview, render, Text } from "@react-email/components";
import type { CSSProperties } from "react";
import { brand, font } from "../src/theme.ts";

const nebula = { amber: "#bf795c", rose: "#8a668b", violet: "#6c5682" };

const looks = {
  dark: {
    page: brand.space,
    scheme: "dark",
    headline: "#ffffff",
    strong: "#eef0fa",
    ink2: "#aeb4d0",
    ink3: "#8f96b8",
    picture: "assets/figure-night.jpg",
    // Dissolves into the night on every side, so the figure has no frame.
    figure: {
      backgroundSize: "100% auto",
      backgroundPosition: "top center",
      padding: "34px 40px 30px",
    },
  },
  light: {
    page: brand.moonlight,
    scheme: "light",
    headline: brand.navy,
    strong: brand.navy,
    ink2: "#4a5175",
    ink3: "#5f6789",
    picture: "assets/figure-light.jpg",
    // Shown as it is: a rounded window of night set into the light page.
    figure: {
      backgroundSize: "cover",
      backgroundPosition: "center",
      borderRadius: 24,
      padding: "22px 22px 22px",
    },
  },
} as const;
type Look = (typeof looks)[keyof typeof looks];

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

function SignInCode({
  l,
  email,
  code,
  expiresIn,
}: {
  l: Look;
  email: string;
  code: string;
  expiresIn: string;
}) {
  // The legacy attributes reach mail apps that ignore CSS backgrounds; on the rounded cell itself.
  const legacy = { background: l.picture, bgcolor: brand.space } as Record<string, string>;
  return (
    <Html lang="en">
      <Head>
        <meta name="color-scheme" content={l.scheme} />
        <meta name="supported-color-schemes" content={l.scheme} />
        <link
          rel="stylesheet"
          href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700&display=swap"
        />
        {/* On phones the dark picture is shorter, so the code moves up to stay on it. */}
        <style>
          {"@media (max-width:480px){.pic-gap{height:20px!important;line-height:20px!important}}"}
        </style>
      </Head>
      <Preview>Enter it on the sign-in page to continue.</Preview>
      <Body style={{ margin: 0, backgroundColor: l.page, fontFamily: font }}>
        <Container style={{ maxWidth: 600, padding: "52px 32px 40px" }}>
          <Wordmark size={22} color={l.strong} />

          <Text style={{ ...headline, color: l.headline }}>Your sign-in code is ready.</Text>
          <Text style={{ ...sub, color: l.ink2 }}>
            Enter it on the sign-in page to continue as{" "}
            <span style={{ color: l.strong }}>{email}</span>.
          </Text>

          {/* The figure: the picture, with the code on smoky glass over it. */}
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
                  {...legacy}
                  style={{
                    backgroundColor: brand.space,
                    backgroundImage: `url(${l.picture})`,
                    backgroundRepeat: "no-repeat",
                    ...l.figure,
                  }}
                >
                  <Text style={chip}>Sign-in code</Text>
                  <div
                    className={l.scheme === "dark" ? "pic-gap" : undefined}
                    style={{ height: 96, lineHeight: "96px", fontSize: 1 }}
                  >
                    &nbsp;
                  </div>
                  <div style={glass}>
                    <Text style={codeStyle}>{code}</Text>
                    <Text style={expires}>
                      <span style={waiting} />
                      Expires in {expiresIn}
                    </Text>
                  </div>
                </td>
              </tr>
            </tbody>
          </table>

          <Text style={{ ...caption, color: l.strong }}>Didn't try to sign in?</Text>
          <Text style={{ ...sub, marginTop: 6, color: l.ink2 }}>
            You can ignore this email. Nobody gets in without the code.
          </Text>

          <div style={nebulaLine} />
          <Wordmark size={17} color={l.strong} />
          <Text style={{ ...foot, marginTop: 12, color: l.ink3 }}>
            Sent to {email}. Questions? Write to{" "}
            <span style={{ color: l.ink2 }}>support@oryxa.in</span>.
          </Text>
          <Text style={{ ...foot, marginTop: 4, color: l.ink3 }}>Oryxa AI Private Limited</Text>
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
// The waiting dot: the ember stays for the wordmark and Needs you only.
const waiting: CSSProperties = {
  display: "inline-block",
  width: 7,
  height: 7,
  borderRadius: 4,
  margin: "0 8px 1px 0",
  backgroundColor: "#a3b2f5",
};
const headline: CSSProperties = {
  margin: "56px 0 0",
  fontSize: 40,
  lineHeight: "42px",
  fontWeight: 600,
  letterSpacing: "-0.03em",
};
const sub: CSSProperties = { margin: "18px 0 0", fontSize: 17, lineHeight: "27px" };
// Smoky glass reads over both the bright ground and the dark sky; mail apps cannot blur.
const glass: CSSProperties = {
  backgroundColor: "rgba(4,5,15,0.42)",
  border: "1px solid rgba(255,255,255,0.22)",
  borderRadius: 18,
  padding: "20px 24px",
};
const chip: CSSProperties = {
  ...glass,
  display: "inline-block",
  margin: 0,
  padding: "5px 12px",
  borderRadius: 9999,
  fontSize: 12,
  lineHeight: "16px",
  fontWeight: 500,
  color: "#eef0fa",
};
const codeStyle: CSSProperties = {
  margin: 0,
  fontSize: 38,
  lineHeight: "42px",
  fontWeight: 600,
  letterSpacing: "0.32em",
  fontVariantNumeric: "tabular-nums",
  color: "#ffffff",
};
const expires: CSSProperties = {
  margin: "10px 0 0",
  fontSize: 13,
  lineHeight: "18px",
  fontWeight: 500,
  color: "rgba(255,255,255,0.78)",
};
const caption: CSSProperties = {
  margin: "36px 0 0",
  fontSize: 20,
  lineHeight: "26px",
  fontWeight: 600,
  letterSpacing: "-0.015em",
};
// The nebula's gas colours together, never one alone, as a drawn accent.
const nebulaLine: CSSProperties = {
  height: 2,
  lineHeight: "2px",
  fontSize: 1,
  margin: "48px 0 28px",
  backgroundColor: nebula.rose,
  backgroundImage: `linear-gradient(90deg, ${nebula.amber} 0%, ${nebula.rose} 50%, ${nebula.violet} 100%)`,
};
const foot: CSSProperties = { margin: 0, fontSize: 13, lineHeight: "19px" };

await mkdir("preview", { recursive: true });
for (const [name, l] of Object.entries(looks)) {
  await writeFile(
    `preview/prototype-picture-${name}.html`,
    await render(<SignInCode l={l} email="priya@acme.com" code="482913" expiresIn="10 minutes" />),
  );
}
