/** The brand mark (docs/brand/logo-mark.svg) inline, so it follows the theme colours. */
/** 文 from Source Han Sans Bold (SIL OFL 1.1), converted to an outline — see docs/brand/LICENSES.md. */
const GLYPH = "M526.8 331.2 476.9 346C485.9 362.8 494.8 385 499.1 401H333.4V446.6H395C416.1 501.6 443.4 548.8 478.5 587.7C437.9 619.3 387.2 641.6 326 656.8C335 667.7 349.4 689.5 354.5 700.8C417.2 682.5 469.9 656.4 513.2 621.3C554.5 656 604.8 681.7 666.4 698.1C673.8 685.2 687.5 665 698 654.4C639.1 641.2 590 617.8 549.4 587C584.1 549.1 610.7 502.7 630.5 446.6H690.6V401H520.6L553.7 390.4C549.4 374.1 537.7 349.5 526.8 331.2ZM513.9 555C483.9 524.2 460.5 487.5 443.4 446.6H578.3C562.3 489.5 541.2 525.4 513.9 555Z";

export function Logo({ size = 22 }: { size?: number }) {
  return (
    <svg className="logo" width={size} height={size} viewBox="232 232 560 560" aria-hidden="true">
      <path
        className="brackets"
        d="M262 392V320a58 58 0 0 1 58-58h72M632 262h72a58 58 0 0 1 58 58v72M762 632v72a58 58 0 0 1-58 58h-72M392 762h-72a58 58 0 0 1-58-58v-72"
        fill="none"
        strokeWidth="48"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path className="glyph" d={GLYPH} />
    </svg>
  );
}

export function Brand({ size = 22, name = "OCR Trans" }: { size?: number; name?: string }) {
  return (
    <div className="brand">
      <Logo size={size} />
      <span>{name}</span>
    </div>
  );
}
