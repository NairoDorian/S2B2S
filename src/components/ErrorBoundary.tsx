import { Errored } from "solid-js";
import type { JSX } from "@solidjs/web";

interface ErrorBoundaryProps {
  children: JSX.Element;
  context: string;
}

export const ErrorBoundary = (props: ErrorBoundaryProps) => {
  return (
    <Errored
      fallback={(error) => {
        console.error(`Error rendering ${props.context}:`, error());
        return null;
      }}
    >
      {props.children}
    </Errored>
  );
};
