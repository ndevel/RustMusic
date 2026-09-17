import { Download, X, Trash2 } from "lucide-react";
import { useStore } from "../store";
import type { DownloadState } from "../types";

function fmtBytes(b: number): string {
  if (b === 0) return "—";
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
}

function DownloadItem({ item }: { item: DownloadState }) {
  const cancelDownload = useStore((s) => s.cancelDownload);
  return (
    <div className="flex items-center gap-3 h-12 px-2.5 rounded-xl hover:bg-[var(--shade-hover)] transition-colors group">
      <div className="relative w-9 h-9 rounded-lg overflow-hidden shrink-0 flex items-center justify-center bg-[rgba(243,233,216,0.05)]">
        <Download size={14} className="text-[var(--accent)]" />
        {item.downloading && (
          <svg className="absolute inset-0 w-9 h-9 -rotate-90">
            <circle
              cx="18"
              cy="18"
              r="15"
              fill="none"
              stroke="var(--accent)"
              strokeWidth="2.5"
              strokeDasharray={`${(item.pct / 100) * 94.25} 94.25`}
              strokeLinecap="round"
              className="transition-all duration-300"
            />
          </svg>
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-[12.5px] text-[var(--ink)] truncate">{item.title}</div>
        <div className="text-[11px] text-[var(--ink-3)] mt-0.5">
          {item.total > 0
            ? `${fmtBytes(item.received)} / ${fmtBytes(item.total)}  ${item.pct}%`
            : item.downloading
              ? `已下载 ${fmtBytes(item.received)}`
              : "完成"}
        </div>
      </div>
      {item.downloading && (
        <button
          className="btn-ghost w-7 h-7 opacity-0 group-hover:opacity-100 shrink-0"
          onClick={() => cancelDownload(item.id)}
          title="取消下载"
        >
          <X size={13} />
        </button>
      )}
    </div>
  );
}

export default function DownloadsPanel() {
  const downloads = useStore((s) => s.downloads);
  const downloadsOpen = useStore((s) => s.downloadsOpen);
  const setDownloadsOpen = useStore((s) => s.setDownloadsOpen);
  const cancelDownload = useStore((s) => s.cancelDownload);

  if (!downloadsOpen) return null;

  return (
    <aside className="fixed bottom-[88px] right-6 z-[61] w-[340px] max-h-[min(64vh,520px)] flex flex-col glass-strong rounded-2xl shadow-2xl anim-menu overflow-hidden">
      <div className="h-14 px-5 flex items-center justify-between shrink-0">
        <span className="text-[13.5px] font-semibold flex items-center gap-2.5 text-[var(--ink)]">
          <Download size={15} className="text-[var(--accent)]" />
          下载列表
          <span className="text-[11.5px] text-[var(--ink-3)] font-normal">
            {downloads.length} 个
          </span>
        </span>
        <div className="flex items-center gap-1">
          {downloads.some((d) => d.downloading) && (
            <button
              className="btn-ghost w-8 h-8"
              onClick={() => {
                for (const d of downloads) {
                  if (d.downloading) cancelDownload(d.id);
                }
              }}
              title="取消全部"
            >
              <Trash2 size={14} />
            </button>
          )}
          <button className="btn-ghost w-8 h-8" onClick={() => setDownloadsOpen(false)}>
            <X size={15} />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-2.5 py-1.5">
        {downloads.length === 0 && (
          <div className="text-[12.5px] text-[var(--ink-3)] text-center pt-10">没有下载任务</div>
        )}
        {downloads.map((item) => (
          <DownloadItem key={item.id} item={item} />
        ))}
      </div>
    </aside>
  );
}
