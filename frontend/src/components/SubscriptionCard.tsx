import React, { useState } from "react";
import { buildPauseTx, buildResumeTx } from "../stellar";
import { friendlyError } from "../utils/errors";

interface SubscriptionCardProps {
  subscription: {
    merchant: string;
    amount: string;
    interval: number;
    last_charged: number;
    active: boolean;
    paused: boolean;
    referrer: string | null;
    label: string;
  };
  userKey: string;
  onCancel: () => void;
  onPause: (xdr: string) => Promise<string>;
  onRefresh: () => void;
}

function formatInterval(secs: number): string {
  if (secs >= 2_592_000) return `${Math.round(secs / 2_592_000)}mo`;
  if (secs >= 604_800) return `${Math.round(secs / 604_800)}w`;
  if (secs >= 86_400) return `${Math.round(secs / 86_400)}d`;
  return `${secs}s`;
}

export default function SubscriptionCard({
  subscription,
  userKey,
  onCancel,
  onPause,
  onRefresh,
}: SubscriptionCardProps) {
  const { merchant, amount, interval, last_charged, active, paused, referrer, label } = subscription;
  const nextCharge = new Date(
    (last_charged + interval) * 1000
  ).toLocaleDateString();
  const xlm = (Number(amount) / 10_000_000).toFixed(7);
  const [pauseLoading, setPauseLoading] = useState(false);
  const [resumeLoading, setResumeLoading] = useState(false);
  const [showPauseConfirm, setShowPauseConfirm] = useState(false);
  const [pauseStatus, setPauseStatus] = useState<string | null>(null);

  async function handlePause() {
    setPauseStatus(null);
    setPauseLoading(true);
    try {
      const xdr = await buildPauseTx(userKey);
      const hash = await onPause(xdr);
      setPauseStatus(`Paused! tx: ${hash.slice(0, 12)}…`);
      onRefresh();
      setShowPauseConfirm(false);
    } catch (e: unknown) {
      const rawMessage = e instanceof Error ? e.message : String(e);
      setPauseStatus(`Error: ${friendlyError(rawMessage)}`);
    } finally {
      setPauseLoading(false);
    }
  }

  async function handleResume() {
    setPauseStatus(null);
    setResumeLoading(true);
    try {
      const xdr = await buildResumeTx(userKey);
      const hash = await onPause(xdr);
      setPauseStatus(`Resumed! tx: ${hash.slice(0, 12)}…`);
      onRefresh();
    } catch (e: unknown) {
      const rawMessage = e instanceof Error ? e.message : String(e);
      setPauseStatus(`Error: ${friendlyError(rawMessage)}`);
    } finally {
      setResumeLoading(false);
    }
  }

  const statusBadgeClass = paused ? "badge-paused" : active ? "badge-active" : "badge-inactive";
  const statusText = paused ? "Paused" : active ? "Active" : "Cancelled";

  return (
    <div className="card">
      <div className="subscription-card__header">
        <div>
          <h2 className="subscription-card__title">{label}</h2>
          <small className="subscription-card__subtitle">to {merchant.slice(0, 8)}…{merchant.slice(-6)}</small>
        </div>
        <span className={`badge ${statusBadgeClass}`}>
          {statusText}
        </span>
      </div>

      <div className="subscription-rows">
        <Row label="Amount" value={`${xlm} XLM`} />
        <Row label="Interval" value={formatInterval(interval)} />
        <Row label="Next charge" value={active && !paused ? nextCharge : "—"} />
        {referrer && (
          <Row label="Referrer" value={`${referrer.slice(0, 8)}…${referrer.slice(-6)}`} />
        )}
      </div>

      <div className="subscription-card__actions">
        {active && !paused && (
          <>
            <button onClick={() => setShowPauseConfirm(true)} className="btn-secondary pause-btn">
              Pause
            </button>
            <button onClick={onCancel} className="btn-danger cancel-btn">
              Cancel
            </button>
          </>
        )}
        {active && paused && (
          <>
            <button onClick={handleResume} disabled={resumeLoading} className="btn-primary resume-btn">
              {resumeLoading ? "Resuming…" : "Resume"}
            </button>
            <button onClick={onCancel} className="btn-danger cancel-btn">
              Cancel
            </button>
          </>
        )}
      </div>

      {showPauseConfirm && (
        <div className="modal-overlay" onClick={() => setShowPauseConfirm(false)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <h3>Pause subscription?</h3>
            <p>You won't be charged while paused. You can resume anytime.</p>
            <div className="modal-actions">
              <button onClick={() => setShowPauseConfirm(false)} className="btn-secondary">
                Cancel
              </button>
              <button onClick={handlePause} disabled={pauseLoading} className="btn-primary">
                {pauseLoading ? "Pausing…" : "Pause"}
              </button>
            </div>
          </div>
        </div>
      )}

      {pauseStatus && (
        <p
          className="form-status"
          style={{
            color: pauseStatus.startsWith("Error") ? "var(--color-danger)" : "var(--color-success)",
          }}
        >
          {pauseStatus}
        </p>
      )}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="subscription-row">
      <span className="subscription-row__label">{label}</span>
      <span className="subscription-row__value">{value}</span>
    </div>
  );
}
