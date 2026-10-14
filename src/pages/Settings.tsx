import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { api, ProviderPreset, ProviderView, SettingsView } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { WorkspacePicker } from "@/components/WorkspacePicker";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { toast } from "sonner";
import {
  ArrowLeft,
  CheckCircle2,
  FolderOpen,
  Loader2,
  PlugZap,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";

interface EditState {
  id: string;
  name: string;
  baseUrl: string;
  apiKey: string;
  defaultModel: string;
  /** 模型上下文窗口（R3：窗口用量指示的数据源） */
  contextWindow: number;
}

export function Settings() {
  const navigate = useNavigate();
  const [view, setView] = useState<SettingsView | null>(null);
  const [presets, setPresets] = useState<ProviderPreset[]>([]);
  const [addOpen, setAddOpen] = useState(false);
  const [presetId, setPresetId] = useState("deepseek");
  const [edit, setEdit] = useState<EditState | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [testing, setTesting] = useState(false);
  /** D56：默认工作区（作品库根目录） */
  const [wsRoot, setWsRoot] = useState("");
  const [wsSaving, setWsSaving] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);

  async function saveWorkspace() {
    if (!wsRoot.trim() || wsSaving) return;
    setWsSaving(true);
    try {
      const normalized = await api.setWorksRoot(wsRoot.trim());
      setWsRoot(normalized);
      toast.success(`默认工作区已保存：${normalized}`);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setWsSaving(false);
    }
  }

  async function pickWorkspace() {
    setPickerOpen(true);
  }

  async function refresh() {
    try {
      const [v, p] = await Promise.all([
        api.getSettings(),
        api.listProviderPresets(),
      ]);
      setView(v);
      setPresets(p);
    } catch (e) {
      toast.error(String(e));
    }
  }

  useEffect(() => {
    refresh();
    api.getWorksRoot().then(setWsRoot).catch(() => {});
  }, []);

  function startAdd() {
    const preset = presets.find((p) => p.id === presetId) ?? presets[0];
    setEdit({
      id: `${preset.id}-${Date.now().toString(36).slice(-4)}`,
      name: preset.name,
      baseUrl: preset.base_url,
      apiKey: "",
      defaultModel: preset.fallback_model,
      contextWindow: preset.context_window,
    });
    setModels([]);
    setAddOpen(false);
  }

  function startEdit(p: ProviderView) {
    setEdit({
      id: p.id,
      name: p.name,
      baseUrl: p.base_url,
      apiKey: "",
      defaultModel: p.default_model,
      contextWindow: p.context_window,
    });
    setModels([]);
  }

  async function fetchModels() {
    if (!edit) return;
    setLoadingModels(true);
    try {
      // 先保存当前输入（key 空则沿用已存），再拉列表
      await api.upsertProvider(
        edit.id,
        edit.name,
        edit.baseUrl,
        edit.apiKey,
        edit.defaultModel,
        false,
        edit.contextWindow,
      );
      setEdit({ ...edit, apiKey: "" });
      const list = await api.listModels(edit.baseUrl, "");
      setModels(list);
      if (list.length === 0) toast.info("该端点未返回模型列表，请手填");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setLoadingModels(false);
    }
  }

  async function handleSave(activate: boolean) {
    if (!edit) return;
    try {
      await api.upsertProvider(
        edit.id,
        edit.name,
        edit.baseUrl,
        edit.apiKey,
        edit.defaultModel,
        activate,
        edit.contextWindow,
      );
      setEdit(null);
      await refresh();
      toast.success("已保存");
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function handleTest() {
    if (!edit) return;
    setTesting(true);
    try {
      await api.upsertProvider(
        edit.id,
        edit.name,
        edit.baseUrl,
        edit.apiKey,
        edit.defaultModel,
        false,
        edit.contextWindow,
      );
      setEdit({ ...edit, apiKey: "" });
      const r = await api.testProvider("", "", "");
      toast.success(`连通：${r.slice(0, 60)}`);
    } catch (e) {
      toast.error(`失败：${String(e).slice(0, 120)}`);
    } finally {
      setTesting(false);
    }
  }

  async function handleRemove(id: string) {
    try {
      await api.removeProvider(id);
      await refresh();
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function handleActivate(id: string) {
    try {
      await api.setActiveProvider(id);
      await refresh();
    } catch (e) {
      toast.error(String(e));
    }
  }

  return (
    /* D56：固定头 + 内容区独立滚动（原来 min-h-screen 在定高容器内会溢出窗口） */
    <div className="flex h-full flex-col bg-background">
      <header className="flex h-12 shrink-0 items-center gap-3 border-b px-4">
        <Button variant="ghost" size="sm" className="h-8 px-2" onClick={() => navigate(-1)}>
          <ArrowLeft data-icon="inline-start" />
          返回
        </Button>
        <span className="text-sm font-semibold">设置</span>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto w-full max-w-2xl p-6">
        {/* 工作区（D56）：默认作品库根目录——新建作品默认落于此 */}
        <Card className="mb-4">
          <CardHeader>
            <CardTitle className="text-base">工作区</CardTitle>
            <CardDescription>
              默认作品库文件夹：新建作品默认创建在这里，作品库页面按它列出作品。
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="flex items-center gap-2">
              <Input
                value={wsRoot}
                onChange={(e) => setWsRoot(e.currentTarget.value)}
                placeholder="例如：D:\novels"
                className="min-w-0 flex-1 font-mono text-xs"
                aria-label="默认工作区"
              />
              <Button
                type="button"
                variant="outline"
                size="icon"
                aria-label="浏览文件夹"
                title="浏览文件夹"
                onClick={pickWorkspace}
              >
                <FolderOpen className="size-4" />
              </Button>
              <Button
                type="button"
                size="sm"
                disabled={!wsRoot.trim() || wsSaving}
                onClick={saveWorkspace}
              >
                {wsSaving && <Loader2 data-icon="inline-start" className="animate-spin" />}
                保存
              </Button>
            </div>
          </CardContent>
        </Card>

        <div className="mb-4 flex items-center justify-between">
          <p className="text-xs text-muted-foreground">
            key 只保存在本机 Rust 侧，界面永不回显明文。勾选"启用"的供应商用于全部
            Agent（可在下方按 Agent 覆盖模型）。
          </p>
        </div>

        <div className="flex flex-col gap-3">
          {view?.providers.length === 0 && (
            <Card>
              <CardContent className="flex flex-col items-center gap-2 p-10 text-muted-foreground">
                <p className="text-sm">还没有供应商，先添加一个</p>
              </CardContent>
            </Card>
          )}
          {view?.providers.map((p) => (
            <Card key={p.id} className={p.id === view.active_provider ? "border-primary/50" : ""}>
              <CardHeader>
                <CardTitle className="text-base">{p.name}</CardTitle>
                <CardDescription className="font-mono text-xs">
                  {p.base_url}
                </CardDescription>
                <CardAction className="flex items-center gap-2">
                  {p.id === view.active_provider ? (
                    <Badge>
                      <CheckCircle2 data-icon="inline-start" />
                      启用中
                    </Badge>
                  ) : (
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-7 text-xs"
                      onClick={() => handleActivate(p.id)}
                    >
                      启用
                    </Button>
                  )}
                </CardAction>
              </CardHeader>
              <CardContent className="flex items-center gap-2 text-xs text-muted-foreground">
                <span className="font-mono">{p.default_model || "未设模型"}</span>
                {!p.no_key && <span>· {p.masked_key}</span>}
                <div className="ml-auto flex gap-1">
                  <Button variant="ghost" size="sm" className="h-7 px-2 text-xs" onClick={() => startEdit(p)}>
                    编辑
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs text-destructive"
                    onClick={() => handleRemove(p.id)}
                  >
                    <Trash2 data-icon="inline-start" />
                    删除
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>

        <div className="mt-4 flex items-center gap-2">
          <Select value={presetId} onValueChange={(v) => v && setPresetId(v)}>
            <SelectTrigger className="w-56">
              <SelectValue placeholder="选择供应商预设" />
            </SelectTrigger>
            <SelectContent>
              {presets.map((p) => (
                <SelectItem key={p.id} value={p.id}>
                  {p.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Button onClick={() => setAddOpen(true)}>
            <Plus data-icon="inline-start" />
            添加供应商
          </Button>
        </div>

        {/* 添加确认（选预设后点添加直接进编辑） */}
        <Dialog open={addOpen} onOpenChange={setAddOpen}>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>添加供应商</DialogTitle>
              <DialogDescription>
                将以当前选择的预设开始配置，可修改全部字段。
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button variant="outline" onClick={() => setAddOpen(false)}>
                取消
              </Button>
              <Button onClick={startAdd}>开始配置</Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* 编辑对话框 */}
        <Dialog open={edit !== null} onOpenChange={(o) => !o && setEdit(null)}>
          <DialogContent className="max-w-lg">
            <DialogHeader>
              <DialogTitle>{edit?.id.includes("-") ? "编辑供应商" : "配置供应商"}</DialogTitle>
              <DialogDescription>
                OpenAI 兼容端点。可先获取模型列表再选择默认模型。
              </DialogDescription>
            </DialogHeader>
            {edit && (
              <FieldGroup>
                <Field>
                  <FieldLabel htmlFor="p-name">名称</FieldLabel>
                  <Input
                    id="p-name"
                    value={edit.name}
                    onChange={(e) => setEdit({ ...edit, name: e.currentTarget.value })}
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="p-url">Base URL</FieldLabel>
                  <Input
                    id="p-url"
                    value={edit.baseUrl}
                    onChange={(e) => setEdit({ ...edit, baseUrl: e.currentTarget.value })}
                    placeholder="https://api.deepseek.com/v1"
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="p-key">API Key</FieldLabel>
                  <Input
                    id="p-key"
                    type="password"
                    value={edit.apiKey}
                    onChange={(e) => setEdit({ ...edit, apiKey: e.currentTarget.value })}
                    placeholder={
                      view?.providers.some((p) => p.id === edit.id)
                        ? "已保存，留空则不修改"
                        : "sk-…"
                    }
                  />
                </Field>
                <Field>
                  <FieldLabel htmlFor="p-model">默认模型</FieldLabel>
                  <div className="flex gap-2">
                    {models.length > 0 ? (
                      <Select
                        value={edit.defaultModel}
                        onValueChange={(v) => setEdit({ ...edit, defaultModel: v ?? "" })}
                      >
                        <SelectTrigger id="p-model" className="flex-1">
                          <SelectValue placeholder="选择模型" />
                        </SelectTrigger>
                        <SelectContent>
                          {models.map((m) => (
                            <SelectItem key={m} value={m}>
                              {m}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    ) : (
                      <Input
                        id="p-model"
                        value={edit.defaultModel}
                        onChange={(e) => setEdit({ ...edit, defaultModel: e.currentTarget.value })}
                        placeholder="deepseek-chat"
                        className="flex-1"
                      />
                    )}
                    <Button
                      variant="outline"
                      onClick={fetchModels}
                      disabled={loadingModels || !edit.baseUrl.trim()}
                    >
                      {loadingModels ? (
                        <Loader2 data-icon="inline-start" className="animate-spin" />
                      ) : (
                        <RefreshCw data-icon="inline-start" />
                      )}
                      获取列表
                    </Button>
                  </div>
                </Field>
                <Field>
                  <FieldLabel htmlFor="p-window">上下文窗口（tokens）</FieldLabel>
                  <Input
                    id="p-window"
                    value={String(edit.contextWindow)}
                    onChange={(e) =>
                      setEdit({
                        ...edit,
                        contextWindow:
                          Number(e.currentTarget.value.replace(/[^0-9]/g, "")) || 0,
                      })
                    }
                    placeholder="128000"
                  />
                </Field>
              </FieldGroup>
            )}
            <DialogFooter className="sm:justify-between">
              <Button variant="outline" onClick={handleTest} disabled={testing}>
                {testing ? (
                  <Loader2 data-icon="inline-start" className="animate-spin" />
                ) : (
                  <PlugZap data-icon="inline-start" />
                )}
                测试连接
              </Button>
              <div className="flex gap-2">
                <Button variant="outline" onClick={() => handleSave(false)}>
                  保存
                </Button>
                <Button onClick={() => handleSave(true)}>保存并启用</Button>
              </div>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* 应用内文件夹选择器（D56） */}
        <WorkspacePicker
          open={pickerOpen}
          onOpenChange={setPickerOpen}
          initialPath={wsRoot}
          onPick={setWsRoot}
        />
        </div>
      </div>
    </div>
  );
}
