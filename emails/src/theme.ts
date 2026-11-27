// Oryxa Website Language tokens, as email needs them: hex values only, no CSS variables.
export const color = {
  space: "#04050f",
  moonlight: "#f3f4fa",
  navy: "#151b36",
  card: "#ffffff",
  subtle: "#eceef7",
  line: "#e1e4f0",
  ink2: "#4a5175",
  ink3: "#5f6789",
  nightInk: "#eef0fa",
  nightInk3: "#8f96b8",
  nightLine: "#1a1e42",
  nebulaSky: "#161228",
  sea: "#34498a",
  ember: "#d98e62",
} as const;

export const font = `"Plus Jakarta Sans", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Helvetica, Arial, sans-serif`;

export type Scene =
  | "moon-night-nebula-v2"
  | "moon-rise-v2"
  | "sky-veil-v2"
  | "moon-earth-v2"
  | "moon-dawn-v2";
