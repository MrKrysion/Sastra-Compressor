// A segmented control. krysion.js draws the sliding pill.
interface Props<T extends string | number> {
  label: string;
  value: T;
  options: { value: T; label: string; tip?: string }[];
  onChange(v: T): void;
  disabled?: boolean;
}

export function Seg<T extends string | number>({ label, value, options, onChange, disabled }: Props<T>) {
  return (
    <div className="seg block" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={String(o.value)}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          aria-pressed={o.value === value}
          data-tip={o.tip}
          disabled={disabled}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
