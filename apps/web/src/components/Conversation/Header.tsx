type Props = {
  workspaceName: string | null;
  threadTitle: string | null;
};

export default function Header({ workspaceName, threadTitle }: Props) {
  if (!workspaceName && !threadTitle) return null;
  return (
    <div className="web-chat-header">
      {workspaceName && (
        <>
          <span>{workspaceName}</span>
          <span className="web-chat-header-sep">/</span>
        </>
      )}
      {threadTitle && <span>{threadTitle}</span>}
    </div>
  );
}
