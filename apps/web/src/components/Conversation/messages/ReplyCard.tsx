import type { ReplyCard as ReplyCardData } from "../../../utils/replyCards";
import MapReplyCard from "./MapReplyCard";
import ReportReplyCard from "./ReportReplyCard";

export default function ReplyCard({ card }: { card: ReplyCardData }) {
  switch (card.kind) {
    case "map.v3":
      return <MapReplyCard card={card} />;
    case "report.v1":
      return <ReportReplyCard card={card} />;
  }
}
