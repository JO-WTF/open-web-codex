import CornerDownRight from "lucide-react/dist/esm/icons/corner-down-right";
import Trash2 from "lucide-react/dist/esm/icons/trash-2";

export type QueuedFollowUp = {
  id: string;
  text: string;
};

type Props = {
  items: QueuedFollowUp[];
  canSteer: boolean;
  steeringId: string | null;
  onSteer: (id: string) => void;
  onDelete: (id: string) => void;
};

export default function FollowUpQueue({ items, canSteer, steeringId, onSteer, onDelete }: Props) {
  if (items.length === 0) return null;

  return (
    <div className="web-followup-queue" aria-label="Queued follow-up messages">
      {items.map((item, index) => (
        <div className="web-followup-item" key={item.id} style={{ zIndex: items.length - index }}>
          <CornerDownRight size={14} aria-hidden="true" />
          <span>{item.text}</span>
          <button
            type="button"
            className="web-followup-steer"
            disabled={!canSteer || steeringId !== null}
            onClick={() => onSteer(item.id)}
            aria-label={`Steer now: ${item.text}`}
          >
            <CornerDownRight size={13} aria-hidden="true" />
            {steeringId === item.id ? "Steering…" : "Steer"}
          </button>
          <button type="button" className="web-followup-delete" onClick={() => onDelete(item.id)} aria-label={`Delete queued message: ${item.text}`}>
            <Trash2 size={14} aria-hidden="true" />
          </button>
        </div>
      ))}
    </div>
  );
}
