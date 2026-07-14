import { useEffect, useMemo, useState } from "react";
import Check from "lucide-react/dist/esm/icons/check";
import CircleHelp from "lucide-react/dist/esm/icons/circle-help";
import type { RequestUserInputRequest, RequestUserInputResponse } from "../../../types";

type Props = {
  request: RequestUserInputRequest;
  submitting: boolean;
  onSubmit: (request: RequestUserInputRequest, response: RequestUserInputResponse) => void;
};

export default function UserInputCard({ request, submitting, onSubmit }: Props) {
  const [selections, setSelections] = useState<Record<string, number | null>>({});
  const [notes, setNotes] = useState<Record<string, string>>({});

  useEffect(() => {
    setSelections(Object.fromEntries(request.params.questions.map((question) => [question.id, null])));
    setNotes(Object.fromEntries(request.params.questions.map((question) => [question.id, ""])));
  }, [request]);

  const complete = useMemo(() => request.params.questions.every((question) => {
    const hasSelection = selections[question.id] != null;
    const hasText = Boolean(notes[question.id]?.trim());
    return question.options?.length ? hasSelection || (question.isOther && hasText) : hasText;
  }), [notes, request.params.questions, selections]);

  const submit = () => {
    const answers: RequestUserInputResponse["answers"] = {};
    request.params.questions.forEach((question) => {
      const values: string[] = [];
      const selected = selections[question.id];
      if (selected != null) {
        const option = question.options?.[selected];
        const value = option?.label || option?.description;
        if (value) values.push(value);
      }
      const note = notes[question.id]?.trim();
      if (note) values.push(question.options?.length ? `user_note: ${note}` : note);
      answers[question.id] = { answers: values };
    });
    onSubmit(request, { answers });
  };

  return (
    <div className="web-user-input-card" role="group" aria-label="Input requested">
      <div className="web-user-input-title"><CircleHelp size={16} aria-hidden="true" /><span>Input requested</span></div>
      {request.params.questions.map((question) => (
        <section className="web-user-input-question" key={question.id}>
          {question.header ? <div className="web-user-input-header">{question.header}</div> : null}
          <div className="web-user-input-prompt">{question.question}</div>
          {question.options?.length ? (
            <div className="web-user-input-options">
              {question.options.map((option, index) => {
                const selected = selections[question.id] === index;
                return (
                  <button type="button" className={`web-user-input-option${selected ? " is-selected" : ""}`} key={`${question.id}-${option.label}-${index}`} onClick={() => setSelections((current) => ({ ...current, [question.id]: index }))}>
                    <span className="web-user-input-check">{selected ? <Check size={13} aria-hidden="true" /> : null}</span>
                    <span><strong>{option.label}</strong>{option.description ? <small>{option.description}</small> : null}</span>
                  </button>
                );
              })}
            </div>
          ) : null}
          {question.isOther || !question.options?.length ? (
            question.isSecret ? (
              <input type="password" className="web-user-input-note" value={notes[question.id] ?? ""} onChange={(event) => setNotes((current) => ({ ...current, [question.id]: event.target.value }))} placeholder="Type your answer" />
            ) : (
              <textarea className="web-user-input-note" value={notes[question.id] ?? ""} onChange={(event) => setNotes((current) => ({ ...current, [question.id]: event.target.value }))} placeholder={question.options?.length ? "Other answer or notes (optional)" : "Type your answer"} rows={2} />
            )
          ) : null}
        </section>
      ))}
      <div className="web-user-input-actions">
        <button type="button" disabled={!complete || submitting} onClick={submit}>{submitting ? "Submitting…" : "Submit"}</button>
      </div>
    </div>
  );
}
