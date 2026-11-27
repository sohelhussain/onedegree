export function LogoMark({ size = 44 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 48 48"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <rect
        x="2"
        y="2"
        width="44"
        height="44"
        rx="14"
        fill="#121620"
        stroke="#273045"
        strokeWidth="2"
      />
      {/* Baseline horizon */}
      <path
        d="M11 32H37"
        stroke="#475569"
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray="3 3"
      />
      {/* 1-degree rising trajectory arc */}
      <path
        d="M11 32C20 31.5 29 26 36.5 14.5"
        stroke="#34D399"
        strokeWidth="3"
        strokeLinecap="round"
      />
      {/* Degree beacon dot */}
      <circle cx="36.5" cy="14.5" r="3.2" fill="#F5B942" />
      <circle
        cx="21"
        cy="15"
        r="2.5"
        stroke="#F4F6FB"
        strokeWidth="1.6"
      />
    </svg>
  );
}
