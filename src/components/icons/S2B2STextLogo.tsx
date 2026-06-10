const S2B2STextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width || 320}
      height={height || 48}
      className={className}
      viewBox="0 0 320 48"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <text
        x="0"
        y="40"
        fontFamily="system-ui, -apple-system, sans-serif"
        fontSize="40"
        fontWeight="bold"
        fill="currentColor"
        className="fill-text"
      >
        S2B2S
      </text>
    </svg>
  );
};

export default S2B2STextLogo;
