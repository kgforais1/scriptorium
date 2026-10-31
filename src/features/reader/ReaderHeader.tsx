import { useEffect, useRef } from "react";

type Props = {
  title: string;
  /** The page indicator text, such as "pages 4–5 of 202". */
  indicator: string;
  closing: boolean;
  onClose: () => void;
};

/** The top of the reader: the book title, the page indicator, and **Close book**. */
export function ReaderHeader({ title, indicator, closing, onClose }: Readonly<Props>) {
  const titleRef = useRef<HTMLHeadingElement>(null);
  // Focus the title when the book opens, so the change is announced and Tab starts here.
  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  return (
    <header className="reader__header">
      <h1 className="reader__title" title={title} ref={titleRef} tabIndex={-1}>
        {title}
      </h1>
      <output className="reader__indicator" aria-live="polite">
        {indicator}
      </output>
      <button type="button" className="reader__button reader__button--text" disabled={closing} onClick={onClose}>
        Close book
      </button>
    </header>
  );
}
