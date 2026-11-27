import { Body, Button, Container, Head, Html, Link, Preview, Text } from "@react-email/components";
import type { CSSProperties, ReactNode } from "react";
import { brand, font, nebula, type Scene, useField } from "./theme.ts";

const SUPPORT = "support@oryxa.in";

/** Legacy HTML attributes some mail apps still read where they ignore CSS. */
const attrs = (a: Record<string, string>) => a as Record<string, string>;

/**
 * Wordmark, a split header (one idea, one sentence), one figure (the picture with the email's
 * key detail on glass over it), the action, a caption, then a nebula hairline and the footer.
 * Dark lets the picture dissolve into the night; light shows it as a rounded window.
 */
export function Layout(props: {
  assets: string;
  /** The document title, for screen readers and the browser tab of "view online". */
  title: string;
  preview: string;
  scene: Scene;
  headline: ReactNode;
  sub: ReactNode;
  /** The small glass label at the top of the figure. */
  chip: ReactNode;
  /** The glass panel at the foot of the figure. */
  feature: ReactNode;
  caption: { title: ReactNode; body: ReactNode };
  /** Who the email went to, shown in the footer. */
  to: string;
  /** What follows the figure: the action, details. */
  children?: ReactNode;
}) {
  const f = useField();
  const picture = `${props.assets}/${props.scene}-${f.name === "dark" ? "night" : "light"}.jpg`;
  return (
    <Html lang="en">
      <Head>
        <title>{props.title}</title>
        {/* Stops iOS turning codes, dates and addresses into blue links. */}
        <meta name="format-detection" content="telephone=no,date=no,address=no,email=no,url=no" />
        {/* Declared so mail apps keep these colours instead of inverting them. */}
        <meta name="color-scheme" content={f.scheme} />
        <meta name="supported-color-schemes" content={f.scheme} />
        {/* One variable file covers 400 to 700; apps without web fonts use the system sans. */}
        <link
          rel="stylesheet"
          href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400..700&display=swap"
        />
        <style>
          {
            // On phones the dark picture is shorter, so the glass moves up to stay on it.
            "@media (max-width:480px){.fig-gap{height:20px!important;line-height:20px!important}}" +
              // Links iOS still detects take the surrounding colour instead of default blue.
              "a[x-apple-data-detectors]{color:inherit!important;text-decoration:none!important}"
          }
        </style>
      </Head>
      <Preview>{props.preview}</Preview>
      <Body style={{ margin: 0, backgroundColor: f.page, fontFamily: font }}>
        {/* Gmail drops <body> styles, so the page colour also lives on a full-width wrapper. */}
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
                    {props.headline}
                  </Text>
                  <Text style={{ ...sub, color: f.ink2 }}>{props.sub}</Text>

                  <Figure picture={picture} chip={props.chip}>
                    {props.feature}
                  </Figure>

                  {props.children}

                  <Text style={{ ...caption, color: f.strong }}>{props.caption.title}</Text>
                  <Text style={{ ...sub, marginTop: 6, color: f.ink2 }}>{props.caption.body}</Text>

                  <div style={nebulaLine} />
                  <Wordmark size={17} color={f.strong} />
                  <Text style={{ ...foot, marginTop: 12, color: f.ink3 }}>
                    Sent to {props.to}. Questions? Write to{" "}
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

/** The picture with glass over it. The VML markers become Outlook's background in the build. */
function Figure({
  picture,
  chip,
  children,
}: {
  picture: string;
  chip: ReactNode;
  children: ReactNode;
}) {
  const f = useField();
  return (
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
            {...attrs({ background: picture, bgcolor: brand.space })}
            style={{
              backgroundColor: brand.space,
              backgroundImage: `url(${picture})`,
              backgroundRepeat: "no-repeat",
              ...f.figure,
            }}
          >
            <div data-vml-open={picture} data-vml-rounded={String(f.rounded)} />
            <Text style={chipStyle}>{chip}</Text>
            <div
              data-skip-in-text="true"
              className={f.rounded ? undefined : "fig-gap"}
              style={{ height: 96, lineHeight: "96px", fontSize: 1 }}
            >
              &nbsp;
            </div>
            <div style={glass}>{children}</div>
            <div data-vml-close={String(f.rounded)} />
          </td>
        </tr>
      </tbody>
    </table>
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

/** The gold ember dot: only for the wordmark and for what needs the person. */
export function Ember() {
  return <span style={{ ...dot, width: 7, height: 7, borderRadius: 4, margin: "0 7px 1px 0" }} />;
}

/** A one-time code. One tap selects all of it, so it copies cleanly. */
export function GlassCode({ code, expiresIn }: { code: string; expiresIn: string }) {
  return (
    <>
      <Text style={codeStyle}>
        <span style={{ userSelect: "all", WebkitUserSelect: "all" }}>{code}</span>
      </Text>
      <Expires in={expiresIn} />
    </>
  );
}

/** A small label over one strong line, and when it expires. */
export function GlassNote({
  label,
  value,
  expiresIn,
}: {
  label: ReactNode;
  value: ReactNode;
  expiresIn: string;
}) {
  return (
    <>
      <Text
        style={{ margin: 0, fontSize: 13, lineHeight: "18px", color: "rgba(255,255,255,0.72)" }}
      >
        {label}
      </Text>
      <Text
        style={{
          margin: "4px 0 0",
          fontSize: 21,
          lineHeight: "27px",
          fontWeight: 600,
          letterSpacing: "-0.015em",
          color: "#ffffff",
          wordBreak: "break-word",
        }}
      >
        {value}
      </Text>
      <Expires in={expiresIn} />
    </>
  );
}

function Expires(props: { in: string }) {
  return (
    <Text style={expires}>
      {/* The waiting dot: the ember stays for the wordmark and Needs you only. */}
      <span style={waiting} />
      Expires in {props.in}
    </Text>
  );
}

/** The one action, with its full link written under it for apps that drop buttons. */
export function Action({ href, children }: { href: string; children: ReactNode }) {
  const f = useField();
  return (
    <>
      <div style={{ margin: "32px 0 0" }}>
        <Button
          href={href}
          style={{
            display: "inline-block",
            backgroundColor: f.buttonFill,
            color: f.buttonLabel,
            borderRadius: 9999,
            padding: "14px 24px",
            fontSize: 15,
            lineHeight: "15px",
            fontWeight: 600,
            letterSpacing: "-0.01em",
            textDecoration: "none",
          }}
        >
          {children}
          <span data-skip-in-text="true">&nbsp;&nbsp;→</span>
        </Button>
      </div>
      {/* Plain text already gives the link with the button's words, so this stays out of it. */}
      <Text
        data-skip-in-text="true"
        style={{ margin: "20px 0 0", fontSize: 13, lineHeight: "20px", color: f.ink3 }}
      >
        Button not working? Paste this link into your browser:
        <br />
        <Link
          href={href}
          style={{ color: f.link, textDecoration: "underline", wordBreak: "break-all" }}
        >
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
      className="ledger"
      style={{ margin: "32px 0 0" }}
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
// Smoky glass reads over both bright ground and dark sky; mail apps cannot blur.
const glass: CSSProperties = {
  backgroundColor: "rgba(4,5,15,0.42)",
  border: "1px solid rgba(255,255,255,0.22)",
  borderRadius: 18,
  padding: "20px 24px",
};
const chipStyle: CSSProperties = {
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
  margin: "40px 0 0",
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
