import React, { useState } from "react";
import { buildSubscribeTx } from "../stellar";
import { friendlyError } from "../utils/errors";

interface Props {
  userKey: string;
  onSign: (xdr: string) => Promise<string>;
  onSuccess: () => void;
}

const INTERVALS = [
  { label: "Daily", value: 86_400 },
  { label: "Weekly", value: 604_800 },
  { label: "Monthly (~30d)", value: 2_592_000 },
];

const NATIVE_TOKEN = "GBUQWP3BOUZX34ULNQG23RQ6F4YUSXHTQSXUSMGB45OQNCTMWQMOTMA2";

function isValidStellarAddress(addr: string): boolean {
  return /^G[A-Z0-9]{55}$/.test(addr);
}

export default function SubscribeForm({ userKey, onSign, onSuccess }: Props) {
  const [merchant, setMerchant] = useState("");
  const [amount, setAmount] = useState("");
  const [interval, setInterval] = useState(INTERVALS[2].value);
  const [label, setLabel] = useState("");
  const [referrer, setReferrer] = useState("");
  const [referrerError, setReferrerError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  function validateReferrer(value: string): string | null {
    if (!value) return null; // Optional field
    if (!isValidStellarAddress(value)) {
      return "Invalid Stellar address format";
    }
    return null;
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setStatus(null);
    setReferrerError(null);
    setLoading(true);

    try {
      // Validate referrer if provided
      if (referrer) {
        const err = validateReferrer(referrer);
        if (err) {
          setReferrerError(err);
          setLoading(false);
          return;
        }
      }

      // Convert XLM → stroops (1 XLM = 10_000_000)
      const stroops = BigInt(Math.round(parseFloat(amount) * 10_000_000));
      const xdr = await buildSubscribeTx(
        userKey,
        merchant,
        stroops,
        BigInt(interval),
        NATIVE_TOKEN,
        referrer || null,
        label || "Untitled"
      );
      const hash = await onSign(xdr);
      setStatus(`Subscribed! tx: ${hash.slice(0, 12)}…`);
      onSuccess();
    } catch (e: unknown) {
      const rawMessage = e instanceof Error ? e.message : String(e);
      setStatus(`Error: ${friendlyError(rawMessage)}`);
    } finally {
      setLoading(false);
    }
  }

  return (
    <form onSubmit={handleSubmit} className="subscribe-form">
      <h2 className="subscribe-form__title">New Subscription</h2>

      <label className="form-group">
        <span className="form-label">Merchant address</span>
        <input
          placeholder="G…"
          value={merchant}
          onChange={(e) => setMerchant(e.target.value)}
          required
        />
      </label>

      <label className="form-group">
        <span className="form-label">Amount (XLM per period)</span>
        <input
          type="number"
          min="0.0000001"
          step="0.0000001"
          placeholder="5"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          required
        />
      </label>

      <label className="form-group">
        <span className="form-label">Billing interval</span>
        <select value={interval} onChange={(e) => setInterval(Number(e.target.value))}>
          {INTERVALS.map((i) => (
            <option key={i.value} value={i.value}>
              {i.label}
            </option>
          ))}
        </select>
      </label>

      <label className="form-group">
        <span className="form-label">Subscription name</span>
        <input
          type="text"
          placeholder="e.g., Netflix subscription"
          maxLength={50}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
        />
        <small className="form-hint">{label.length}/50 characters</small>
      </label>

      <label className="form-group">
        <span className="form-label">Referral address (optional)</span>
        <input
          placeholder="G… (leave blank for no referral)"
          value={referrer}
          onChange={(e) => {
            setReferrer(e.target.value);
            if (e.target.value) {
              setReferrerError(validateReferrer(e.target.value));
            } else {
              setReferrerError(null);
            }
          }}
        />
        {referrerError && (
          <small className="form-error">{referrerError}</small>
        )}
        {!referrerError && referrer && (
          <small className="form-success">✓ Valid address</small>
        )}
        <small className="form-hint">Optional: track referrals to this address</small>
      </label>

      <button type="submit" disabled={loading || !!referrerError} className="btn-primary subscribe-form__submit">
        {loading ? "Signing…" : "Subscribe"}
      </button>

      {status && (
        /* Dynamic: color is error/success state-driven — inline color is intentional */
        <p
          className="form-status"
          style={{
            color: status.startsWith("Error") ? "var(--color-danger)" : "var(--color-success)",
          }}
        >
          {status}
        </p>
      )}
    </form>
  );
}
