export function CoinIcon({ size = 20 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <circle cx="12" cy="12" r="10.5" fill="#F5B942" stroke="#B87C14" strokeWidth="1.5" />
      <circle cx="12" cy="12" r="7.5" stroke="#FEF08A" strokeOpacity="0.6" strokeWidth="1" />
      {/* Degree mark inside gold coin */}
      <circle cx="12" cy="12" r="3" stroke="#1A1405" strokeWidth="2.2" />
    </svg>
  );
}
