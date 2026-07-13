import { useRef, useEffect } from "react";

type Props = {
  draft: string;
  onDraftChange: (text: string) => void;
  onSend: () => void;
  busy: boolean;
  disabled: boolean;
};

export default function Composer({ draft, onDraftChange, onSend, busy, disabled }: Props) {
  const textRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (textRef.current) {
      textRef.current.style.height = "auto";
      textRef.current.style.height = `${Math.min(textRef.current.scrollHeight, 160)}px`;
    }
  }, [draft]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!disabled && !busy) onSend();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e);
    }
  };

  return (
    <form className="web-composer" onSubmit={handleSubmit}>
      <div className="web-composer-inner">
        <textarea
          ref={textRef}
          value={draft}
          onChange={(e) => onDraftChange(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Ask Codex to perform a task in the selected workspace..."
          disabled={busy}
          rows={1}
        />
      </div>
      <button
        type="submit"
        className="web-send-btn"
        disabled={disabled || busy}
        aria-label="Send"
      >
        <svg viewBox="0 0 16 16" fill="none" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
          <path d="M2 8l-1-5 13 5L1 13l1-5zm0 0h12" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
      </button>
    </form>
  );
}
