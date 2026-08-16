import React, {
  createContext,
  forwardRef,
  useContext,
  useId,
  useState,
} from "react";

interface TabsContextValue {
  value: string;
  onValueChange: (value: string) => void;
  name: string;
}

const TabsContext = createContext<TabsContextValue | undefined>(undefined);

function useTabsContext() {
  const ctx = useContext(TabsContext);
  if (!ctx) {
    throw new Error("Tabs primitive: use a <Tabs> parent.");
  }
  return ctx;
}

export interface TabsProps {
  value?: string;
  defaultValue?: string;
  onValueChange?: (value: string) => void;
  children: React.ReactNode;
  className?: string;
}

export const Tabs = forwardRef<HTMLDivElement, TabsProps>(
  ({ value, defaultValue, onValueChange, children, className }, ref) => {
    const id = useId();
    const [internalValue, setInternalValue] = useState(defaultValue ?? "");
    const controlled = value !== undefined;
    const currentValue = controlled ? value! : internalValue;
    const handleChange = (next: string) => {
      if (!controlled) setInternalValue(next);
      onValueChange?.(next);
    };

    return (
      <TabsContext.Provider
        value={{ value: currentValue, onValueChange: handleChange, name: id }}
      >
        <div ref={ref} className={className} data-tabs-root>
          {children}
        </div>
      </TabsContext.Provider>
    );
  },
);
Tabs.displayName = "Tabs";

export interface TabsListProps {
  children: React.ReactNode;
  className?: string;
}

export const TabsList = forwardRef<HTMLDivElement, TabsListProps>(
  ({ children, className }, ref) => (
    <div
      ref={ref}
      role="tablist"
      className={`inline-flex h-10 items-center justify-start rounded-lg bg-mid-gray/10 p-1 text-muted-foreground border border-mid-gray/40 overflow-x-auto scrollbar-hide ${className ?? ""}`}
    >
      {children}
    </div>
  ),
);
TabsList.displayName = "TabsList";

export interface TabsTriggerProps {
  value: string;
  children: React.ReactNode;
  className?: string;
  icon?: React.ReactNode;
}

export const TabsTrigger = forwardRef<HTMLButtonElement, TabsTriggerProps>(
  ({ value, children, className, icon }, ref) => {
    const { value: currentValue, onValueChange, name } = useTabsContext();
    const active = currentValue === value;
    return (
      <button
        ref={ref}
        id={`${name}-tab-${value.replace(/\s+/g, "")}`}
        role="tab"
        aria-selected={active}
        aria-controls={`${name}-panel-${value.replace(/\s+/g, "")}`}
        type="button"
        className={`inline-flex items-center justify-center whitespace-nowrap rounded-md px-3 py-1.5 text-sm font-medium transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50 ${
          active
            ? "bg-logo-primary/15 text-logo-primary shadow-sm"
            : "hover:bg-mid-gray/30 hover:text-text"
        } ${className ?? ""}`}
        onClick={() => {
          onValueChange(value);
        }}
      >
        {icon}
        {children}
      </button>
    );
  },
);
TabsTrigger.displayName = "TabsTrigger";

export interface TabsContentProps {
  value: string;
  children: React.ReactNode;
  className?: string;
}

export const TabsContent = forwardRef<HTMLDivElement, TabsContentProps>(
  ({ value, children, className }, ref) => {
    const { value: currentValue, name } = useTabsContext();
    const active = currentValue === value;
    return (
      <div
        ref={ref}
        id={`${name}-panel-${value.replace(/\s+/g, "")}`}
        role="tabpanel"
        aria-labelledby={`${name}-tab-${value.replace(/\s+/g, "")}`}
        data-state={active ? "active" : "inactive"}
        className={`${active ? "block" : "hidden"} ${className ?? ""}`}
      >
        {children}
      </div>
    );
  },
);
TabsContent.displayName = "TabsContent";
