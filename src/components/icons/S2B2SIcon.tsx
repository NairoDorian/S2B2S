const S2B2SIcon = ({
  width,
  height,
}: {
  width?: number | string;
  height?: number | string;
}) => (
  <svg
    width={width || 32}
    height={height || 32}
    viewBox="0 0 32 32"
    className="fill-text stroke-text"
    xmlns="http://www.w3.org/2000/svg"
  >
    <circle
      cx="16"
      cy="16"
      r="14"
      stroke="currentColor"
      strokeWidth="2"
      fill="none"
    />
    <text
      x="16"
      y="21"
      textAnchor="middle"
      fontFamily="system-ui, -apple-system, sans-serif"
      fontSize="10"
      fontWeight="bold"
      fill="currentColor"
    >
      S2
    </text>
  </svg>
);

export default S2B2SIcon;
