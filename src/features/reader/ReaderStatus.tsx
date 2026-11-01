type Props = {
  /** The book has no pages, so there is no engine. */
  empty: boolean;
  /** The first spread is still decoding. */
  loading: boolean;
  /** Why the engine could not start, such as missing WebGL2. It replaces the book. */
  error: string | null;
};

/** The empty, loading, and start-error states, drawn in the middle of the screen. */
export function ReaderStatus({ empty, loading, error }: Readonly<Props>) {
  if (empty) return <p className="reader__status">This book has no pages.</p>;
  if (error !== null) {
    return (
      <p className="reader__status reader__status--error" role="alert">
        {error}
      </p>
    );
  }
  if (loading) return <output className="reader__status reader__status--loading">Preparing the vellum…</output>;
  return null;
}
