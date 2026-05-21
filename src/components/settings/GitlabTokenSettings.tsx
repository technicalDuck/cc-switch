import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Save, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { skillsApi } from "@/lib/api";

interface TokenRow {
  host: string;
  token: string;
  /** 后端是否已经存有该 host 的真实 token（决定下次保存时是否使用占位逻辑） */
  saved: boolean;
  /** Token 字段当前显示的是否为掩码（"********"） */
  masked: boolean;
  dirty: boolean;
}

const MASKED_PLACEHOLDER = "********";

function newEmptyRow(): TokenRow {
  return { host: "", token: "", saved: false, masked: false, dirty: true };
}

/**
 * GitLab Personal Access Token 管理
 *
 * - 后端按 host 存储 PAT，前端读到的 token 已被掩码为 "********"
 * - 用户修改某行 token（脱离掩码）后才会真正提交新的 token
 * - 仅删除一行时直接调用 removeGitlabToken
 */
export function GitlabTokenSettings() {
  const { t } = useTranslation();
  const [rows, setRows] = useState<TokenRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [savingHost, setSavingHost] = useState<string | null>(null);

  const refresh = async () => {
    try {
      setLoading(true);
      const tokens = await skillsApi.getGitlabTokens();
      const next: TokenRow[] = Object.entries(tokens || {}).map(
        ([host, token]) => ({
          host,
          token: token || MASKED_PLACEHOLDER,
          saved: true,
          masked: true,
          dirty: false,
        }),
      );
      setRows(next);
    } catch (e) {
      console.error("Failed to load gitlab tokens", e);
      toast.error(t("settings.gitlabTokens.loadFailed"));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const updateRow = (index: number, patch: Partial<TokenRow>) => {
    setRows((prev) => {
      const next = prev.slice();
      next[index] = { ...next[index], ...patch, dirty: true };
      return next;
    });
  };

  const handleAddRow = () => {
    setRows((prev) => [...prev, newEmptyRow()]);
  };

  const handleSave = async (index: number) => {
    const row = rows[index];
    const host = row.host.trim();
    if (!host) {
      toast.error(t("settings.gitlabTokens.hostRequired"));
      return;
    }
    if (!row.token || row.token === MASKED_PLACEHOLDER) {
      toast.error(t("settings.gitlabTokens.tokenRequired"));
      return;
    }
    try {
      setSavingHost(host);
      await skillsApi.setGitlabToken(host, row.token);
      toast.success(t("settings.gitlabTokens.saveSuccess", { host }));
      await refresh();
    } catch (e) {
      console.error("Failed to save gitlab token", e);
      toast.error(t("settings.gitlabTokens.saveFailed"), {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setSavingHost(null);
    }
  };

  const handleRemove = async (index: number) => {
    const row = rows[index];
    if (!row.saved) {
      setRows((prev) => prev.filter((_, i) => i !== index));
      return;
    }
    try {
      setSavingHost(row.host);
      await skillsApi.removeGitlabToken(row.host);
      toast.success(t("settings.gitlabTokens.removeSuccess", { host: row.host }));
      await refresh();
    } catch (e) {
      console.error("Failed to remove gitlab token", e);
      toast.error(t("settings.gitlabTokens.removeFailed"), {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setSavingHost(null);
    }
  };

  const isEmpty = useMemo(() => !loading && rows.length === 0, [loading, rows]);

  return (
    <section className="space-y-3">
      <header className="space-y-1">
        <h3 className="text-sm font-medium">
          {t("settings.gitlabTokens.title")}
        </h3>
        <p className="text-xs text-muted-foreground">
          {t("settings.gitlabTokens.description")}
        </p>
      </header>

      <div className="space-y-2">
        {loading ? (
          <p className="text-xs text-muted-foreground">
            {t("common.loading", { defaultValue: "Loading..." })}
          </p>
        ) : null}

        {!loading && isEmpty ? (
          <p className="text-xs text-muted-foreground">
            {t("settings.gitlabTokens.empty")}
          </p>
        ) : null}

        {rows.map((row, index) => (
          <div
            key={`${row.host}-${index}`}
            className="flex flex-col gap-2 rounded-lg border border-border-default bg-background p-3 sm:flex-row sm:items-end"
          >
            <div className="flex-1 space-y-1">
              <Label
                htmlFor={`gitlab-host-${index}`}
                className="text-xs text-muted-foreground"
              >
                {t("settings.gitlabTokens.hostLabel")}
              </Label>
              <Input
                id={`gitlab-host-${index}`}
                value={row.host}
                placeholder="gitlab.corp.com"
                disabled={row.saved}
                onChange={(e) => updateRow(index, { host: e.target.value })}
                className="h-9"
              />
            </div>
            <div className="flex-1 space-y-1">
              <Label
                htmlFor={`gitlab-token-${index}`}
                className="text-xs text-muted-foreground"
              >
                {t("settings.gitlabTokens.tokenLabel")}
              </Label>
              <Input
                id={`gitlab-token-${index}`}
                type="password"
                value={row.token}
                placeholder={t("settings.gitlabTokens.tokenPlaceholder")}
                onFocus={() => {
                  if (row.masked) {
                    updateRow(index, { token: "", masked: false });
                  }
                }}
                onChange={(e) =>
                  updateRow(index, { token: e.target.value, masked: false })
                }
                className="h-9"
              />
            </div>
            <div className="flex gap-2">
              <Button
                type="button"
                size="sm"
                onClick={() => handleSave(index)}
                disabled={
                  savingHost === row.host || row.masked || !row.dirty
                }
              >
                <Save className="h-4 w-4 mr-1" />
                {t("common.save")}
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => handleRemove(index)}
                disabled={savingHost === row.host}
                className="hover:text-red-500 hover:bg-red-100 dark:hover:text-red-400 dark:hover:bg-red-500/10"
              >
                <Trash2 className="h-4 w-4" />
              </Button>
            </div>
          </div>
        ))}

        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={handleAddRow}
        >
          <Plus className="h-4 w-4 mr-1" />
          {t("settings.gitlabTokens.addRow")}
        </Button>
      </div>
    </section>
  );
}
