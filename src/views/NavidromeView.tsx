import { useEffect, useMemo, useRef, useState } from "react";
import {
  Loader2,
  MoreHorizontal,
  Play,
  Search,
  Server,
  Unplug,
} from "lucide-react";
import { useStore } from "../store";
import type { CurrentTrack, NdSong } from "../types";
import { fmtTime } from "../utils";
import CoverImg from "../components/CoverImg";

/** 搜索结果列表（含行内播放按钮 / 右键菜单 / 无限加载哨兵） */
function NdResults({
  results,
  searching,
  searched,
  current,
  playing,
  onPlay,
  onMenu,
  sentinel,
}: {
  results: NdSong[];
  searching: boolean;
  searched: boolean;
  current: CurrentTrack | null;
  playing: boolean;
  onPlay: (i: number) => void;
  onMenu: (e: React.MouseEvent, row: NdSong) => void;
  sentinel: React.RefObject<HTMLDivElement>;
}) {
  const togglePlay = useStore((s) => s.togglePlay);
  return (
    <>
      {results.length > 0 && (
        <div className="grid grid-cols-[44px_1fr_1fr_72px_36px] gap-3 px-3 py-2 text-[11px] font-semibold text-[var(--ink-3)] tracking-wide sticky top-0 z-10">
          <span className="text-center">#</span>
          <span>歌曲</span>
          <span>专辑</span>
          <span>时长</span>
          <span />
        </div>
      )}

      {results.map((t, i) => {
        const active = current?.kind === "navidrome" && current.ndid === t.id;
        return (
          <div
            key={`${t.id}-${i}`}
            className={`group grid grid-cols-[44px_1fr_1fr_72px_36px] gap-3 px-3 h-[52px] rounded-xl items-center cursor-pointer transition-colors ${
              active ? "" : "hover:bg-[var(--shade-hover)]"
            }`}
            style={active ? { background: "var(--accent-weak)" } : undefined}
            onDoubleClick={() => onPlay(i)}
            onContextMenu={(e) => onMenu(e, t)}
          >
            <div className="flex items-center justify-center">
              <span
                className={`text-[12px] tabular-nums text-[var(--ink-3)] group-hover:hidden ${
                  active ? "hidden" : ""
                }`}
              >
                {i + 1}
              </span>
              <button
                className={`w-8 h-8 rounded-full items-center justify-center ${
                  active ? "flex" : "hidden group-hover:flex"
                }`}
                style={{ background: "var(--accent)", color: "var(--bg)" }}
                onClick={() => (active && playing ? togglePlay() : onPlay(i))}
                title="播放"
              >
                <Play size={13} fill="currentColor" />
              </button>
            </div>
            <div className="flex items-center gap-3 min-w-0">
              <CoverImg
                src={t.cover}
                seed={t.id}
                iconSize={16}
                className="w-9 h-9 rounded-lg shrink-0"
              />
              <div className="min-w-0">
                <div
                  className={`text-[13px] truncate ${
                    active ? "font-semibold" : "text-[var(--ink)]"
                  }`}
                >
                  {t.title}
                </div>
                <div className="text-[11px] text-[var(--ink-3)] truncate">
                  {t.artist}
                </div>
              </div>
            </div>
            <div className="text-[12px] text-[var(--ink-2)] truncate">{t.album}</div>
            <div className="text-[12px] text-[var(--ink-3)] tabular-nums">
              {fmtTime(t.durationMs / 1000)}
            </div>
            <button
              className="w-8 h-8 rounded-lg flex items-center justify-center text-[var(--ink-3)] opacity-0 group-hover:opacity-100 hover:text-[var(--ink)] transition-opacity"
              onClick={(e) => onMenu(e, t)}
              title="更多操作"
            >
              <MoreHorizontal size={16} />
            </button>
          </div>
        );
      })}

      {searching && !results.length && (
        <div className="flex items-center justify-center gap-2 py-16 text-[13px] text-[var(--ink-3)]">
          <Loader2 size={16} className="animate-spin" />
          正在搜索…
        </div>
      )}

      {!searching && !results.length && searched && (
        <div className="py-16 text-center text-[13px] text-[var(--ink-3)]">
          没有找到匹配的歌曲
        </div>
      )}

      {!searched && !results.length && (
        <div className="py-16 text-center">
          <Server size={36} className="mx-auto text-[var(--ink-3)] mb-3" />
          <div className="text-[13px] text-[var(--ink-2)]">
            输入关键词搜索你 Navidrome 库中的音乐
          </div>
        </div>
      )}

      <div ref={sentinel} className="h-4" />
      {searching && results.length > 0 && (
        <div className="flex justify-center py-3">
          <Loader2 size={16} className="animate-spin text-[var(--ink-3)]" />
        </div>
      )}
    </>
  );
}

export default function NavidromeView() {
  const ndConfigured = useStore((s) => s.ndConfigured);
  const ndServer = useStore((s) => s.ndServer);
  const ndUrl = useStore((s) => s.ndUrl);
  const ndUser = useStore((s) => s.ndUser);
  const ndResults = useStore((s) => s.ndResults);
  const ndSearching = useStore((s) => s.ndSearching);
  const ndSearched = useStore((s) => s.ndSearched);
  const current = useStore((s) => s.current);
  const playing = useStore((s) => s.playing);
  const playNd = useStore((s) => s.playNd);
  const playNext = useStore((s) => s.playNext);
  const addToQueue = useStore((s) => s.addToQueue);
  const ndSearch = useStore((s) => s.ndSearch);
  const ndSaveConfig = useStore((s) => s.ndSaveConfig);
  const ndLogout = useStore((s) => s.ndLogout);
  const toast = useStore((s) => s.toast);

  const [kw, setKw] = useState("");
  const [menu, setMenu] = useState<{ x: number; y: number; row: NdSong } | null>(
    null
  );

  // 配置表单（未连接时显示）
  const [url, setUrl] = useState("");
  const [user, setUser] = useState("");
  const [pass, setPass] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (ndConfigured) {
      setUrl(ndUrl);
      setUser(ndUser);
    }
  }, [ndConfigured, ndUrl, ndUser]);

  // 点击空白处关闭右键菜单
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [menu]);

  const submit = () => ndSearch(kw);

  const save = async () => {
    if (!url.trim() || !user.trim() || !pass.trim()) {
      toast("请填写服务器地址、用户名和密码", "error");
      return;
    }
    setSaving(true);
    try {
      const server = await ndSaveConfig(url, user, pass);
      toast(`已连接 ${server}`, "success");
    } catch (e) {
      toast(String(e), "error");
    } finally {
      setSaving(false);
    }
  };

  // 加载更多：search3 按 offset 分页，结果满 50 条时允许继续加载
  const hasMore = useMemo(() => ndResults.length >= 50, [ndResults]);
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = sentinel.current;
    if (!el || !hasMore) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries[0]?.isIntersecting) ndSearch(kw, true);
      },
      { rootMargin: "200px" }
    );
    io.observe(el);
    return () => io.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasMore, ndSearching, kw]);

  const openMenu = (e: React.MouseEvent, row: NdSong) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu({ x: e.clientX, y: e.clientY, row });
  };

  // ---------- 未连接：连接配置表单 ----------
  if (!ndConfigured) {
    return (
      <div className="flex-1 min-h-0 flex items-center justify-center overflow-y-auto">
        <form
          className="w-[420px] max-w-full px-8 py-10 rounded-3xl"
          style={{
            background: "var(--shade)",
            border: "1px solid var(--shade-hover)",
          }}
          onSubmit={(e) => {
            e.preventDefault();
            save();
          }}
        >
          <div className="flex items-center gap-3 mb-2">
            <div
              className="w-11 h-11 rounded-2xl flex items-center justify-center"
              style={{ background: "var(--accent-weak)" }}
            >
              <Server size={20} style={{ color: "var(--accent)" }} />
            </div>
            <div>
              <div className="text-[16px] font-bold text-[var(--ink)]">
                连接 Navidrome
              </div>
              <div className="text-[11.5px] text-[var(--ink-3)]">
                接入你自建的 Navidrome 音乐库（Subsonic API）
              </div>
            </div>
          </div>

          <label className="block mt-6 mb-1.5 text-[11.5px] font-medium text-[var(--ink-2)]">
            服务器地址
          </label>
          <input
            className="w-full h-10 px-3.5 rounded-xl text-[13px] text-[var(--ink)] bg-[var(--bg)] outline-none"
            style={{ border: "1px solid var(--shade-hover)" }}
            placeholder="http://192.168.1.10:4533"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />

          <label className="block mt-4 mb-1.5 text-[11.5px] font-medium text-[var(--ink-2)]">
            用户名
          </label>
          <input
            className="w-full h-10 px-3.5 rounded-xl text-[13px] text-[var(--ink)] bg-[var(--bg)] outline-none"
            style={{ border: "1px solid var(--shade-hover)" }}
            placeholder="admin"
            value={user}
            onChange={(e) => setUser(e.target.value)}
          />

          <label className="block mt-4 mb-1.5 text-[11.5px] font-medium text-[var(--ink-2)]">
            密码
          </label>
          <input
            type="password"
            className="w-full h-10 px-3.5 rounded-xl text-[13px] text-[var(--ink)] bg-[var(--bg)] outline-none"
            style={{ border: "1px solid var(--shade-hover)" }}
            placeholder="••••••••"
            value={pass}
            onChange={(e) => setPass(e.target.value)}
          />

          <button
            type="submit"
            disabled={saving}
            className="btn-primary w-full mt-6 h-10 rounded-xl flex items-center justify-center gap-2 disabled:opacity-60"
          >
            {saving && <Loader2 size={15} className="animate-spin" />}
            {saving ? "正在连接…" : "连接"}
          </button>

          <div className="mt-4 text-[10.5px] text-[var(--ink-3)] text-center leading-relaxed">
            凭据仅保存在本机设置中，通过 Subsonic API token
            方式认证；请确保服务器可访问。
          </div>
        </form>
      </div>
    );
  }

  // ---------- 已连接：搜索 + 结果列表 ----------
  return (
    <div className="flex-1 min-h-0 flex flex-col">
      {/* 头部：标题块 + 搜索框 + 服务器信息 */}
      <div className="shrink-0 px-7 pt-6 pb-4 flex flex-wrap items-center gap-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2.5">
            <Server size={22} style={{ color: "var(--accent)" }} />
            <span className="text-[21px] font-bold text-[var(--ink)] leading-tight">
              Navidrome
            </span>
            <span className="text-[11px] text-[var(--ink-3)] truncate max-w-[220px]">
              {ndServer || ndUrl}
            </span>
          </div>
          <div className="text-[11.5px] text-[var(--ink-3)] mt-0.5">
            自建音乐库 · {ndUser}
          </div>
        </div>

        <div className="flex-1 min-w-[280px] max-w-[420px] flex items-center gap-2">
          <div
            className="flex-1 h-10 px-4 rounded-xl flex items-center gap-2.5"
            style={{ background: "var(--shade)" }}
          >
            <Search size={15} className="text-[var(--ink-3)] shrink-0" />
            <input
              className="flex-1 bg-transparent outline-none text-[13px] text-[var(--ink)] placeholder:text-[var(--ink-3)]"
              placeholder="搜索歌曲 / 艺术家 / 专辑"
              value={kw}
              onChange={(e) => setKw(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && submit()}
            />
          </div>
          <button
            className="btn-primary h-10 px-5 rounded-xl disabled:opacity-60"
            disabled={ndSearching || !kw.trim()}
            onClick={submit}
          >
            {ndSearching ? (
              <Loader2 size={15} className="animate-spin" />
            ) : (
              "搜索"
            )}
          </button>
        </div>

        <button
          className="btn-ghost h-10 px-3.5 rounded-xl flex items-center gap-2 text-[12.5px] text-[var(--ink-2)]"
          title="断开并清除凭据"
          onClick={() => {
            if (confirm("确定断开 Navidrome 服务器？")) ndLogout();
          }}
        >
          <Unplug size={15} />
          断开
        </button>
      </div>

      {/* 结果列表 */}
      <div className="flex-1 min-h-0 overflow-y-auto px-7 pb-8">
        <NdResults
          results={ndResults}
          searching={ndSearching}
          searched={ndSearched}
          current={current}
          playing={playing}
          onPlay={(i) => playNd(ndResults, i)}
          onMenu={openMenu}
          sentinel={sentinel}
        />
      </div>

      {/* 右键 / 更多菜单 */}
      {menu && (
        <>
          <div className="fixed inset-0 z-[70]" onClick={() => setMenu(null)} />
          <div
            className="fixed z-[71] w-[150px] py-1.5 rounded-xl shadow-xl"
            style={{
              left: menu.x,
              top: menu.y,
              background: "var(--shade-strong)",
              border: "1px solid var(--shade-hover)",
            }}
          >
            {(
              [
                [
                  "play",
                  "播放",
                  () =>
                    playNd(
                      ndResults,
                      Math.max(
                        0,
                        ndResults.findIndex((x) => x.id === menu.row.id)
                      )
                    ),
                ],
                [
                  "next",
                  "下一首播放",
                  () => playNext({ kind: "navidrome", id: menu.row.id }),
                ],
                [
                  "queue",
                  "加入队列",
                  () => addToQueue({ kind: "navidrome", id: menu.row.id }),
                ],
              ] as const
            ).map(([key, label, fn]) => (
              <button
                key={key}
                className="w-full px-3 py-2 text-left text-[12.5px] text-[var(--ink)] hover:bg-[var(--shade-hover)]"
                onClick={() => {
                  fn();
                  setMenu(null);
                }}
              >
                {label}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
