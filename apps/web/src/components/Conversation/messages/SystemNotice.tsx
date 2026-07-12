type Props = {
  text: string;
  variant?: "default" | "success" | "error" | "neutral";
};

export default function SystemNotice({ text, variant = "default" }: Props) {
  return (
    <div className="web-system-notice">
      <div className={`web-system-notice-body web-system-notice-${variant}`}>
        {text}
      </div>
    </div>
  );
}
