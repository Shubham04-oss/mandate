import { createContext, useContext } from "react";

// Oryxa Website Language tokens, as email needs them: hex values only, no CSS variables.
export const brand = {
  space: "#04050f",
  moonlight: "#f3f4fa",
  navy: "#151b36",
  ember: "#d98e62",
  periwinkle: "#a3b2f5",
} as const;

export const nebula = { amber: "#bf795c", rose: "#8a668b", violet: "#6c5682" } as const;

export type Field = "dark" | "light";

/** Each email renders on both fields; everything that differs between them lives here. */
export const fields = {
  dark: {
    page: brand.space,
    scheme: "dark",
    headline: "#ffffff",
    strong: "#eef0fa",
    ink2: "#aeb4d0",
    ink3: "#8f96b8",
    link: brand.periwinkle,
    hairline: "rgba(255,255,255,0.1)",
    buttonFill: "#ffffff",
    buttonLabel: brand.navy,
    // The figure dissolves into the night on every side, so it has no frame.
    figure: {
      backgroundSize: "100% auto",
      backgroundPosition: "top center",
      padding: "34px 40px 30px",
    },
    rounded: false,
  },
  light: {
    page: brand.moonlight,
    scheme: "light",
    headline: brand.navy,
    strong: brand.navy,
    ink2: "#4a5175",
    ink3: "#5f6789",
    link: "#34498a",
    hairline: "rgba(21,27,54,0.1)",
    buttonFill: brand.navy,
    buttonLabel: brand.moonlight,
    // A night sky cannot fade into a light page cleanly, so it is a rounded window set into it.
    figure: {
      backgroundSize: "cover",
      backgroundPosition: "center",
      borderRadius: 24,
      padding: "22px",
    },
    rounded: true,
  },
} as const;

export const FieldContext = createContext<Field>("dark");
export const useField = () => {
  const name = useContext(FieldContext);
  return { name, ...fields[name] };
};

export const font = `"Plus Jakarta Sans", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Helvetica, Arial, sans-serif`;

export type Scene =
  | "moon-night-nebula-v2"
  | "moon-rise-v2"
  | "sky-veil-v2"
  | "moon-earth-v2"
  | "moon-dawn-v2";
