// Oryxa Website Language tokens, as email needs them: hex values only, no CSS variables.
export const color = {
  space: "#04050f",
  navy: "#151b36",
  nightInk: "#eef0fa",
  nightInk2: "#aeb4d0",
  nightInk3: "#8f96b8",
  periwinkle: "#a3b2f5",
  ember: "#d98e62",
} as const;

export const font = `"Plus Jakarta Sans", ui-sans-serif, system-ui, -apple-system, "Segoe UI", Helvetica, Arial, sans-serif`;

export type Scene =
  | "moon-night-nebula-v2"
  | "moon-rise-v2"
  | "sky-veil-v2"
  | "moon-earth-v2"
  | "moon-dawn-v2";
