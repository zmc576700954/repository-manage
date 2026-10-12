import { useEffect, useRef } from 'react';

export interface MenuItem {
  /** Display label. */
  label: string;
  /** Click handler. */
  onClick: () => void;
  /** Render as a non-interactive horizontal separator. */
  separator?: boolean;
  /** Disable the item. */
  disabled?: boolean;
  /** Optional destructive hint — renders the label in red. */
  danger?: boolean;
}

interface Props {
  /** Viewport X (clientX) in CSS px. */
  x: number;
  /** Viewport Y (clientY) in CSS px. */
  y: number;
  items: MenuItem[];
  onClose: () => void;
}

/**
 * Lightweight right-click context menu. Renders at (x, y), closes on outside
 * click, Escape, or after a menu item is selected. Clamps to viewport.
 */
export function ContextMenu({ x, y, items, onClose }: Props) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    // Use mousedown so the menu closes before any subsequent click on a
    // backdrop element (React Flow handles its own onNodeClick etc.).
    document.addEventListener('mousedown', handleDown);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('mousedown', handleDown);
      document.removeEventListener('keydown', handleKey);
    };
  }, [onClose]);

  // Clamp to viewport so the menu never spills offscreen.
  const W = 200;
  const ITEM_H = 32;
  const PAD = 8;
  const totalH = items.reduce((acc, it) => acc + (it.separator ? 9 : ITEM_H), 0) + PAD * 2;
  const maxX = Math.max(0, window.innerWidth - W - 4);
  const maxY = Math.max(0, window.innerHeight - totalH - 4);
  const left = Math.min(x, maxX);
  const top = Math.min(y, maxY);

  return (
    <div
      ref={ref}
      role="menu"
      className="fixed z-50 min-w-[180px] rounded-md border border-gray-200 bg-white py-1 text-sm shadow-lg"
      style={{ left, top, width: W }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((item, idx) => {
        if (item.separator) {
          return <div key={idx} className="my-1 h-px bg-gray-100" role="separator" />;
        }
        return (
          <button
            key={idx}
            role="menuitem"
            disabled={item.disabled}
            onClick={() => {
              if (item.disabled) return;
              onClose();
              // Run after close so the menu unmounts first.
              item.onClick();
            }}
            className={`flex w-full items-center px-3 py-1.5 text-left transition ${
              item.disabled
                ? 'cursor-not-allowed text-gray-300'
                : item.danger
                  ? 'text-red-600 hover:bg-red-50'
                  : 'text-gray-700 hover:bg-gray-100'
            }`}
          >
            {item.label}
          </button>
        );
      })}
    </div>
  );
}
