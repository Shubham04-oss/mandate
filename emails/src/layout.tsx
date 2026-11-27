import { Body, Button, Container, Head, Html, Link, Preview, Text } from "@react-email/components";
import type { CSSProperties, ReactNode } from "react";
import { brand, FieldContext, fields, font, type Scene, useField } from "./theme.ts";

const SUPPORT = "support@oryxa.in";

/**
 * One picture per email, no panels or cards. On night it sits behind the top of the email and
 * fades into the page on every side (baked into the JPG), with the words set straight on it. On
 * light it is shown as it is, as a rounded frame holding the wordmark and the email's one key
 * detail on glass, since a night sky cannot fade into a light page cleanly. Where a mail app drops
 * background images (Outlook on Windows), the same words sit on a plain colour.
 */
export function Layout(props: {
  assets: string;
  preview: string;
  scene: Scene;
  /** Who the email went to, shown in the footer. */
  to: string;
  /** The key detail set on glass inside the picture on light; night shows it in the flow instead. */
  feature: ReactNode;
  children: ReactNode;
}) {
  const f = useField();
  const picture = `${props.assets}/${props.scene}${f.pictureSuffix}.jpg`;
  // The legacy `background` attribute reaches the mail apps that ignore the CSS one.
  const legacy = (
    f.overlay ? { background: picture, bgcolor: f.page } : { bgcolor: f.page }
  ) as Record<string, string>;
  const scheme = f.page === brand.space ? "dark" : "light";
  const footer: CSSProperties = { margin: 0, fontSize: 13, lineHeight: "19px", color: f.ink3 };
  return (
    <Html lang="en">
      <Head>
        {/* Declared so mail apps keep these colours instead of inverting them. */}
        <meta name="color-scheme" content={scheme} />
        <meta name="supported-color-schemes" content={scheme} />
        <link
          rel="stylesheet"
          href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700&display=swap"
        />
      </Head>
      <Preview>{props.preview}</Preview>
      <Body style={{ margin: 0, backgroundColor: f.page, fontFamily: font }}>
        <Container style={{ maxWidth: 600, padding: "16px 0 32px" }}>
          <table width="100%" cellPadding={0} cellSpacing={0} role="presentation" {...legacy}>
            <tbody>
              <tr>
                <td
                  style={{
                    backgroundColor: f.page,
                    ...(f.overlay && {
                      backgroundImage: `url(${picture})`,
                      backgroundSize: "100% auto",
                      backgroundPosition: "top center",
                      backgroundRepeat: "no-repeat",
                    }),
                    padding: "36px 32px 44px",
                  }}
                >
                  {f.overlay ? (
                    <>
                      <Wordmark color={f.wordmark} />
                      {/* Leaves the sky clear before the words begin where the picture fades. */}
                      <div
                        style={{
                          height: f.pictureClear,
                          lineHeight: `${f.pictureClear}px`,
                          fontSize: 1,
                        }}
                      >
                        &nbsp;
                      </div>
                    </>
                  ) : (
                    <Frame picture={picture}>{props.feature}</Frame>
                  )}
                  {props.children}
                  <div
                    style={{
                      height: 1,
                      lineHeight: "1px",
                      fontSize: 1,
                      margin: "44px 0 28px",
                      backgroundColor: f.hairline,
                    }}
                  />
                  <Wordmark color={f.strong} small />
                  <Text style={{ ...footer, marginTop: 12 }}>
                    Sent to {props.to}. Questions? Write to{" "}
                    <Link href={`mailto:${SUPPORT}`} style={{ color: f.ink2 }}>
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

/** The picture as it is, rounded, holding the wordmark and night-coloured glass over it. */
function Frame({ picture, children }: { picture: string; children: ReactNode }) {
  // On the rounded cell itself, so no square corner of colour shows behind the radius.
  const legacy = { background: picture, bgcolor: brand.space } as Record<string, string>;
  return (
    <table
      width="100%"
      cellPadding={0}
      cellSpacing={0}
      role="presentation"
      style={{ marginBottom: 36 }}
    >
      <tbody>
        <tr>
          <td
            {...legacy}
            style={{
              overflow: "hidden",
              backgroundColor: brand.space,
              backgroundImage: `url(${picture})`,
              backgroundSize: "cover",
              backgroundPosition: "center",
              borderRadius: 24,
              padding: "26px 24px 24px",
            }}
          >
            <Wordmark color={fields.night.wordmark} />
            <div style={{ height: 104, lineHeight: "104px", fontSize: 1 }}>&nbsp;</div>
            <FieldContext.Provider value="picture">{children}</FieldContext.Provider>
          </td>
        </tr>
      </tbody>
    </table>
  );
}

/** Shown only on night, where the feature sits in the flow rather than in the picture. */
export function NightOnly({ children }: { children: ReactNode }) {
  return useField().overlay ? children : null;
}

/** A small label over one strong line, on glass. */
export function GlassNote({ label, value }: { label: ReactNode; value: ReactNode }) {
  const f = useField();
  return (
    <div style={{ ...glass(f), margin: "28px 0 0", padding: "14px 18px" }}>
      <Text style={{ margin: 0, fontSize: 13, lineHeight: "18px", color: f.ink2 }}>{label}</Text>
      <Text
        style={{
          margin: "3px 0 0",
          fontSize: 20,
          lineHeight: "26px",
          fontWeight: 600,
          letterSpacing: "-0.015em",
          color: f.headline,
        }}
      >
        {value}
      </Text>
    </div>
  );
}

function Wordmark({ color, small = false }: { color: string; small?: boolean }) {
  const size = small ? 17 : 22;
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

export function Headline({ children }: { children: ReactNode }) {
  const f = useField();
  return (
    <Text
      style={{
        margin: 0,
        fontSize: 32,
        lineHeight: "36px",
        fontWeight: 600,
        letterSpacing: "-0.025em",
        color: f.headline,
      }}
    >
      {children}
    </Text>
  );
}

export function Para({ children }: { children: ReactNode }) {
  const f = useField();
  return (
    <Text style={{ margin: "14px 0 0", fontSize: 16, lineHeight: "25px", color: f.ink2 }}>
      {children}
    </Text>
  );
}

export function Meta({ children }: { children: ReactNode }) {
  const f = useField();
  return <Text style={meta(f.ink3)}>{children}</Text>;
}

/** A one-time code on a strip of glass, large and spaced for reading off one screen into another. */
export function Code({ value }: { value: string }) {
  const f = useField();
  return (
    <div style={{ ...glass(f), margin: "28px 0 0", padding: "20px 24px" }}>
      <Text
        style={{
          margin: 0,
          fontSize: 32,
          lineHeight: "36px",
          fontWeight: 600,
          letterSpacing: "0.3em",
          fontVariantNumeric: "tabular-nums",
          color: f.headline,
        }}
      >
        {value}
      </Text>
    </div>
  );
}

/** The one action, with its link spelled out under it for mail apps that drop buttons. */
export function Action({ href, children }: { href: string; children: ReactNode }) {
  const f = useField();
  return (
    <>
      <div style={{ margin: "30px 0 0" }}>
        <Button
          href={href}
          style={{
            display: "inline-block",
            backgroundColor: f.buttonFill,
            color: f.buttonLabel,
            borderRadius: 9999,
            padding: "13px 22px",
            fontSize: 15,
            lineHeight: "15px",
            fontWeight: 600,
            letterSpacing: "-0.01em",
            textDecoration: "none",
          }}
        >
          {children}&nbsp;&nbsp;→
        </Button>
      </div>
      <Text style={{ ...meta(f.ink3), marginTop: 22 }}>
        Or paste this link into your browser:
        <br />
        <Link href={href} style={{ color: f.link, wordBreak: "break-all" }}>
          {href}
        </Link>
      </Text>
    </>
  );
}

/** Label and value rows split by faint hairlines, straight on the page. */
export function Ledger({ rows }: { rows: [string, ReactNode][] }) {
  const f = useField();
  const cell: CSSProperties = {
    padding: "12px 0",
    borderTop: `1px solid ${f.hairline}`,
    fontSize: 14,
    lineHeight: "20px",
    verticalAlign: "top",
  };
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
            <td style={{ ...cell, color: f.ink3, width: 112 }}>{label}</td>
            <td style={{ ...cell, color: f.strong, fontWeight: 500 }}>{value}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** A glass status pill; the gold ember dot appears only for what needs the person. */
export function NeedsYou() {
  const f = useField();
  return (
    <Text
      style={{
        ...glass(f),
        display: "inline-block",
        margin: "0 0 18px",
        padding: "5px 12px",
        borderRadius: 9999,
        fontSize: 12,
        lineHeight: "16px",
        fontWeight: 500,
        color: f.strong,
      }}
    >
      <span style={{ ...dot, width: 7, height: 7, borderRadius: 4, margin: "0 7px 1px 0" }} />
      Needs you
    </Text>
  );
}

// Translucent fill with a fine edge reads as glass over the picture; mail apps cannot blur.
const glass = (f: { glass: string; glassEdge: string }): CSSProperties => ({
  backgroundColor: f.glass,
  border: `1px solid ${f.glassEdge}`,
  borderRadius: 18,
});
const meta = (color: string): CSSProperties => ({
  margin: "24px 0 0",
  fontSize: 14,
  lineHeight: "21px",
  color,
});
const dot: CSSProperties = {
  display: "inline-block",
  width: "0.2em",
  height: "0.2em",
  marginLeft: "0.1em",
  borderRadius: "0.14em",
  backgroundColor: brand.ember,
};
