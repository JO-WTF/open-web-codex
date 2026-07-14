export default function ThinkingIndicator() {
  return (
    <div className="web-thinking" role="status" aria-live="polite">
      <span className="web-thinking-spinner" aria-hidden="true" />
      <span>Working…</span>
    </div>
  );
}
