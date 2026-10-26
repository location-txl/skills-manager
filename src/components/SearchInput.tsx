import { useTranslation } from "react-i18next";
import { Search, X } from "lucide-react";
import { cn } from "../utils";

/** Shared search input in the app's toolbar style. */
export function SearchInput({
  value,
  onChange,
  placeholder,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  className?: string;
}) {
  const { t } = useTranslation();
  return (
    <div className={cn("relative", className)}>
      <Search className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted" />
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder ?? t("common.search")}
        className="app-input w-full pl-9 pr-8"
      />
      {value ? (
        <button
          onClick={() => onChange("")}
          className="absolute right-2 top-1/2 -translate-y-1/2 rounded-md p-1 text-muted outline-none transition-colors hover:bg-surface-hover hover:text-secondary"
        >
          <X className="h-3 w-3" />
        </button>
      ) : null}
    </div>
  );
}
