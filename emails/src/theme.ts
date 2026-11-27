import { createContext, useContext } from "react";

// Oryxa Website Language tokens, as email needs them: hex values only, no CSS variables.
export const brand = {
  space: "#04050f",
  moonlight: "#f3f4fa",
  navy: "#151b36",
  ember: "#d98e62",
} as const;

export type Field = "night" | "light" | "picture";

/** Each email renders on both fields; everything that differs between them lives here. */
const night = {
  page: brand.space,
  wordmark: "#eef0fa",
  headline: "#ffffff",
  ink2: "#aeb4d0",
  ink3: "#8f96b8",
  strong: "#eef0fa",
  link: "#a3b2f5",
  glass: "rgba(255,255,255,0.07)",
  glassEdge: "rgba(255,255,255,0.14)",
  hairline: "rgba(255,255,255,0.1)",
  buttonFill: "#ffffff",
  buttonLabel: brand.navy,
  pictureSuffix: "",
  // The night picture sits behind the words: 4:3, faded by its last fifth, words over its foot.
  overlay: true,
  pictureClear: 196,
};

// Glass set inside a picture: smoky rather than white, so it reads over bright ground and dark sky.
const picture = {
  ...night,
  glass: "rgba(4,5,15,0.42)",
  glassEdge: "rgba(255,255,255,0.22)",
};

export const fields = {
  night,
  picture,
  light: {
    page: brand.moonlight,
    wordmark: brand.navy,
    headline: brand.navy,
    ink2: "#4a5175",
    ink3: "#5f6789",
    strong: brand.navy,
    link: "#34498a",
    glass: "rgba(255,255,255,0.62)",
    glassEdge: "rgba(21,27,54,0.08)",
    hairline: "rgba(21,27,54,0.1)",
    buttonFill: brand.navy,
    buttonLabel: brand.moonlight,
    pictureSuffix: "-light",
    // A night sky behind dark words would not read: here it is a framed window above them
    // whose foot dissolves into the page.
    overlay: false,
    pictureClear: 0,
  },
} as const;

export const FieldContext = createContext<Field>("night");
export const useField = () => fields[useContext(FieldContext)];

export const font = `"Plus Jakarta Sans", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Helvetica, Arial, sans-serif`;

export type Scene =
  | "moon-night-nebula-v2"
  | "moon-rise-v2"
  | "sky-veil-v2"
  | "moon-earth-v2"
  | "moon-dawn-v2";
