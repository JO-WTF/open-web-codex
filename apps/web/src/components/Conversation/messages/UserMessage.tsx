type Props = {
  text: string;
};

export default function UserMessage({ text }: Props) {
  return (
    <div className="web-msg-user">
      <div className="web-msg-user-body">{text}</div>
    </div>
  );
}
