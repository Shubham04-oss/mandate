export const OTP_LENGTH = 6;

// One-time codes are digits only; pasted codes often carry spaces or a dash.
export function normalizeOtp(input: string): string {
	return input.replace(/\D/g, "").slice(0, OTP_LENGTH);
}
