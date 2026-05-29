import React, { useEffect, useState } from "react";
import { buildCancelTx, buildPayPerUseTx } from "../stellar";
import { friendlyError } from "../utils/errors";
import SubscriptionCard from "./SubscriptionCard";
import SubscriptionCardSkeleton from "./Skeleton";
import { useSubscription } from "../hooks/useSubscription";

interface Props {
  userKey: string;
  onSign: (xdr: string) => Promise<string>;
  refreshTrigger: number;
}

export default function Dashboard({ userKey, onSign, refreshTrigger }: Props) {
  const { subscription: sub, loading, refresh } = useSubscription(userKey, refreshTrigger);
  const [actionStatus, setActionStatus] = useState<string | null>(null);
  const [ppuLoading, setPpuLoading] = useState(false);

  async function handleCancel() {
    setActionStatus(null);
    try {
      const xdr = await buildCancelTx(userKey);
      const hash = await onSign(xdr);
      setActionStatus(`Cancelled. tx: ${hash.slice(0, 12)}…`);
      refresh();
    } catch (e: unknown) {
      const rawMessage = e instanceof Error ? e.message : String(e);
      setActionStatus(`Error: ${friendlyError(rawMessage)}`);
    }
  }

  async function handlePayPerUse(stroops: bigint) {
    setActionStatus(null);
    setPpuLoading(true);
    try {
      const xdr = await buildPayPerUseTx(userKey, stroops);
      const hash = await onSign(xdr);
      setActionStatus(`Paid! tx: ${hash.slice(0, 12)}…`);
      refresh();
    } catch (e: unknown) {
      const rawMessage = e instanceof Error ? e.message : String(e);
      setActionStatus(`Error: ${friendlyError(rawMessage)}`);
    } finally {
      setPpuLoading(false);
    }
  }

  if (loading) return <SubscriptionCardSkeleton />;

  if (!sub) {
    return (
      <div className="card">
        <p className="no-sub-text">No active subscription found.</p>
      </div>
    );
  }

  return (
    <div className="dashboard">
      <SubscriptionCard 
        subscription={sub} 
        userKey={userKey}
        onCancel={handleCancel}
        onPause={onSign}
        onRefresh={refresh}
      />

      {sub.active && !sub.paused && (
        <PayPerUseForm onPay={handlePayPerUse} loading={ppuLoading} />
      )}

      {actionStatus && (
        /* Dynamic: color is error/success state-driven — inline color is intentional */
        <p
          className="action-status"
          style={{
            color: actionStatus.startsWith("Error")
              ? "var(--color-danger)"
              : "var(--color-success)",
          }}
        >
          {actionStatus}
        </p>
      )}
    </div>
  );
}
