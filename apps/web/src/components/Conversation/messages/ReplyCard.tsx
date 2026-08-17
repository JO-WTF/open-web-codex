import type { ReplyCard as ReplyCardData } from "../../../utils/replyCards";
import MapReplyCard from "./MapReplyCard";

export default function ReplyCard({ card, onRevise }: {
  card: ReplyCardData;
  onRevise?: (cardId: string, title: string) => void;
}) {
  return <MapReplyCard card={card} onRevise={onRevise} />;
}
