import type { ReplyCard as ReplyCardData } from "../../../utils/replyCards";
import MapReplyCard from "./MapReplyCard";

export default function ReplyCard({ card }: { card: ReplyCardData }) {
  return <MapReplyCard card={card} />;
}
