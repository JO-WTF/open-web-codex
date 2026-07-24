import type { ReplyCard as ReplyCardData } from "../../../utils/replyCards";
import MapReplyCard from "./MapReplyCard";

export default function ReplyCard({ card }: { card: ReplyCardData }) {
  switch (card.kind) {
    case "map.v2":
      return <MapReplyCard card={card} />;
  }
}
