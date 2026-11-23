import { CheckCircle2, Trash2, XCircle } from "lucide-react";
import { useEffect, useState } from "react";
import { api, type WalletError } from "../lib/api";
import { useT } from "../lib/i18n";
import { CoinIcon } from "./CoinIcon";
import { Button, Field, inputClass, Modal } from "./ui";

export function EditDialog({
  coin,
  current,
  onSave,
  onClose,
}: {
  coin: { id: string; name: string };
  current: string;
  onSave: (id: string, address: string | null) => Promise<void>;
  onClose: () => void;
}) {
  const t = useT();
  const [value, setValue] = useState(current);
  const [state, setState] = useState<{ ok: boolean; msg: string } | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!value.trim()) {
      setState(null);
      return;
    }
    const id = setTimeout(() => {
      api
        .validateCoinWallet(coin.id, value.trim())
        .then(() => setState({ ok: true, msg: t("coinWallet.ok", { c: coin.name }) }))
        .catch((e: WalletError | string) => setState({ ok: false, msg: t(`wallet.err.${typeof e === "string" ? e : e.code}`) }));
    }, 250);
    return () => clearTimeout(id);
  }, [value]);

  const run = async (address: string | null) => {
    setSaving(true);
    await onSave(coin.id, address);
    setSaving(false);
  };

  return (
    <Modal
      open
      onClose={onClose}
      closeLabel={t("confirm.cancel")}
      icon={<CoinIcon id={coin.id} size={40} />}
      title={t("coinWallet.dialog", { c: coin.name })}
      actions={
        <>
          {current && (
            <Button variant="danger" onClick={() => run(null)} disabled={saving} className="mr-auto">
              <Trash2 className="size-4" /> {t("coinWallet.remove")}
            </Button>
          )}
          <Button variant="ghost" onClick={onClose}>
            {t("confirm.cancel")}
          </Button>
          <Button variant="primary" onClick={() => run(value)} loading={saving} disabled={!state?.ok || value.trim() === current}>
            {t("wallet.save")}
          </Button>
        </>
      }
    >
      <p className="mb-4">{t("coinWallet.dialog.body", { c: coin.name })}</p>
      <Field
        label={t("coinWallet.address", { c: coin.name })}
        hint={
          state ? (
            <span className={state.ok ? "text-go" : "text-danger"} role="status">
              {state.ok ? <CheckCircle2 className="mr-1 inline size-3.5" /> : <XCircle className="mr-1 inline size-3.5" />}
              {state.msg}
            </span>
          ) : (
            t("coinWallet.hint")
          )
        }
      >
        <input className={`${inputClass} font-mono text-xs`} value={value} onChange={(e) => setValue(e.target.value)} spellCheck={false} autoComplete="off" />
      </Field>
    </Modal>
  );
}
