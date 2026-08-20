import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { FiBriefcase, FiChevronLeft, FiChevronRight, FiFileText, FiFolder, FiGrid, FiMoreHorizontal, FiPlus, FiRefreshCw, FiSearch, FiTool, FiX } from "react-icons/fi";
import { LuGamepad2, LuPalette } from "react-icons/lu";
import { FaEdge } from "react-icons/fa6";
import { SiFigma, SiGooglechrome, SiNotion } from "react-icons/si";
import { TbBrandAdobePhotoshop } from "react-icons/tb";
import { VscCode } from "react-icons/vsc";

const initialCategories = [
  { id: "work", name: "工作", count: 6, color: "#438dff", icon: FiBriefcase },
  { id: "design", name: "设计", count: 5, color: "#a477ff", icon: LuPalette },
  { id: "play", name: "娱乐", count: 4, color: "#f184aa", icon: LuGamepad2 },
  { id: "tools", name: "工具", count: 7, color: "#49b9aa", icon: FiTool },
  { id: "files", name: "文档", count: 0, color: "#e2a958", icon: FiFileText },
  { id: "folders", name: "文件夹", count: 0, color: "#73a8ff", icon: FiFolder },
];

const apps = [
  { name: "Edge", icon: FaEdge, color: "#22b8bc" },
  { name: "Chrome", icon: SiGooglechrome, color: "#f4bd2f" },
  { name: "Notion", icon: SiNotion, color: "#f5f7fb" },
  { name: "VS Code", icon: VscCode, color: "#39a9ed" },
  { name: "Figma", icon: SiFigma, color: "#a779ff" },
  { name: "Photoshop", icon: TbBrandAdobePhotoshop, color: "#46a7ff" },
];

function inferCategory(item) {
  if (item.kind === "folder") return "folders";
  if (item.kind === "file") return "files";
  const value = item.name.toLowerCase();
  if (/steam|epic|game|游戏|魔兽|battle|wegame|launcher|元宝|梦幻|大话/.test(value)) return "play";
  if (/figma|photoshop|illustrator|blender|design|设计|cocos|unity|unreal|adobe|sketch/.test(value)) return "design";
  if (/zip|everything|terminal|code|tool|工具|控制|dashboard|clash|vlc|driver|录屏|截图|向日葵|trae/.test(value)) return "tools";
  return "work";
}

function useDraggablePosition(storageKey) {
  const [position, setPosition] = useState(() => {
    try { return JSON.parse(localStorage.getItem(storageKey)) ?? null; } catch { return null; }
  });
  function startDrag(event) {
    if (event.button !== 0 || event.target.closest("button, input")) return;
    const surface = event.currentTarget.parentElement;
    const rect = surface.getBoundingClientRect();
    const offsetX = event.clientX - rect.left;
    const offsetY = event.clientY - rect.top;
    document.body.classList.add("dragging-surface");
    const move = (nextEvent) => {
      const left = Math.max(0, Math.min(window.innerWidth - rect.width, nextEvent.clientX - offsetX));
      const top = Math.max(0, Math.min(window.innerHeight - rect.height, nextEvent.clientY - offsetY));
      setPosition({ left, top });
    };
    const finish = (nextEvent) => {
      move(nextEvent);
      document.body.classList.remove("dragging-surface");
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      const left = Math.max(0, Math.min(window.innerWidth - rect.width, nextEvent.clientX - offsetX));
      const top = Math.max(0, Math.min(window.innerHeight - rect.height, nextEvent.clientY - offsetY));
      localStorage.setItem(storageKey, JSON.stringify({ left, top }));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish, { once: true });
  }
  return {
    dragStyle: position ? { left: position.left, top: position.top, right: "auto", transform: "none" } : undefined,
    startDrag,
  };
}

function AddCategory({ onClose, onAdd }) {
  const [name, setName] = useState("");
  const [color, setColor] = useState("#4f8cff");
  const colors = ["#4f8cff", "#9b7cff", "#ef7fa4", "#42b8a3", "#f4a851"];
  function submit(event) { event.preventDefault(); const clean = name.trim(); if (clean) onAdd(clean, color); }
  return <div className="modal-backdrop" onMouseDown={onClose}>
    <form className="new-folder" onSubmit={submit} onMouseDown={(e) => e.stopPropagation()}>
      <header><div><span>新增分类</span><small>创建一个透明文件夹</small></div><button type="button" className="icon-button" onClick={onClose} aria-label="关闭"><FiX /></button></header>
      <label>名称<input autoFocus maxLength={12} value={name} onChange={(e) => setName(e.target.value)} placeholder="例如：学习" /></label>
      <fieldset><legend>颜色</legend><div className="color-row">{colors.map((item) => <button key={item} type="button" aria-label={`选择颜色 ${item}`} className={color === item ? "swatch selected" : "swatch"} style={{ backgroundColor: item }} onClick={() => setColor(item)} />)}</div></fieldset>
      <button className="primary" type="submit" disabled={!name.trim()}><FiPlus />创建分类</button>
    </form>
  </div>;
}

export function App() {
  const [categories, setCategories] = useState(initialCategories);
  const [activeId, setActiveId] = useState("work");
  const [collapsed, setCollapsed] = useState(false);
  const [adding, setAdding] = useState(false);
  const [notice, setNotice] = useState("");
  const [desktopApps, setDesktopApps] = useState([]);
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [scanning, setScanning] = useState(false);
  const folderDrag = useDraggablePosition("qizhuo-folder-position");
  const railDrag = useDraggablePosition("qizhuo-rail-position");
  const isTauri = Boolean(window.__TAURI_INTERNALS__);
  function scanNow(showSuccess = false) {
    if (!isTauri) return;
    setScanning(true);
    invoke("scan_desktop_apps").then((items) => {
      setDesktopApps(items);
      const counts = items.reduce((all, item) => ({ ...all, [inferCategory(item)]: (all[inferCategory(item)] ?? 0) + 1 }), {});
      setCategories((current) => current.map((category) => category.id.startsWith("custom-") ? category : { ...category, count: counts[category.id] ?? 0 }));
      if (showSuccess) { setNotice(`已整理 ${items.length} 个桌面项目`); window.setTimeout(() => setNotice(""), 2200); }
    }).catch(() => setNotice("无法读取桌面快捷方式")).finally(() => setScanning(false));
  }
  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    setCategories((current) => current.map((category) => ({ ...category, count: 0 })));
    scanNow();
  }, []);
  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    let disposed = false;
    let unlisteners = [];
    Promise.all([
      listen("organize-now", () => scanNow(true)),
      listen("add-category-request", () => { setAdding(true); setCollapsed(false); }),
      listen("restore-failed", () => {
        setNotice("桌面图标尚未全部恢复，请在管理员确认中选择“是”");
        window.setTimeout(() => setNotice(""), 5000);
      }),
    ]).then((callbacks) => {
      if (disposed) callbacks.forEach((callback) => callback());
      else unlisteners = callbacks;
    });
    return () => { disposed = true; unlisteners.forEach((callback) => callback()); };
  }, []);
  const active = useMemo(() => categories.find((item) => item.id === activeId) ?? categories[0], [categories, activeId]);
  const normalizedQuery = query.trim().toLowerCase();
  const visibleApps = normalizedQuery ? desktopApps.filter((item) => item.name.toLowerCase().includes(normalizedQuery)) : (isTauri ? desktopApps.filter((item) => inferCategory(item) === activeId) : (activeId === "work" ? apps : []));
  const displayedName = normalizedQuery ? "搜索结果" : active.name;
  const displayedColor = normalizedQuery ? "#8fb5ff" : active.color;
  function addCategory(name, color) {
    const next = { id: `custom-${Date.now()}`, name, count: 0, color, icon: FiFolder };
    setCategories((items) => [...items, next]); setActiveId(next.id); setAdding(false); setCollapsed(false); setNotice(`已创建“${name}”`); window.setTimeout(() => setNotice(""), 2200);
  }
  return <main className="desktop-shell">
    <div className="brand-note"><FiGrid /><span>栖桌正在托盘运行</span></div>
    {!collapsed && active && <section className="folder-panel" style={folderDrag.dragStyle} aria-label={`${displayedName}内容`}>
      <header className="drag-handle" onPointerDown={folderDrag.startDrag}><div><strong style={{ color: displayedColor }}>{displayedName}</strong><span>{visibleApps.length} 个项目</span></div><button className="icon-button" aria-label="更多操作"><FiMoreHorizontal /></button></header>
      {visibleApps.length ? <div className="app-grid">{visibleApps.map(({ name, icon, target, source, color, kind }) => {
        const Icon = typeof icon === "function" ? icon : (kind === "folder" ? FiFolder : (kind === "file" && !icon ? FiFileText : null));
        return <button className="app-tile" key={`${name}-${source ?? target ?? "demo"}`} onDoubleClick={() => target && invoke("open_item", { target })} onContextMenu={(event) => {
          if (!source || !isTauri) return;
          event.preventDefault();
          invoke("show_system_context_menu", { target: source }).catch(() => {
            setNotice("无法打开系统右键菜单");
            window.setTimeout(() => setNotice(""), 2200);
          });
        }} title={target ? `${name}\n双击打开 · 右键显示系统菜单` : name}>{Icon ? <Icon style={{ color }} /> : <img src={icon} alt="" />}<span>{name}</span></button>;
      })}</div> : <button className="empty-folder"><FiPlus /><span>将应用拖到这里</span></button>}
    </section>}
    <aside className={collapsed ? "category-rail collapsed" : "category-rail"} style={collapsed ? undefined : railDrag.dragStyle} aria-label="分类文件夹">
      {collapsed ? <button className="edge-tab" onClick={() => setCollapsed(false)} aria-label="展开栖桌"><span>栖桌</span><FiChevronLeft /></button> : <>
        <div className="rail-head drag-handle" onPointerDown={railDrag.startDrag}><span>分类</span><div className="rail-actions"><button className="icon-button" onClick={() => setSearchOpen((value) => !value)} aria-label="全局搜索"><FiSearch /></button><button className={scanning ? "icon-button spinning" : "icon-button"} onClick={() => scanNow(true)} aria-label="立即整理"><FiRefreshCw /></button><button className="icon-button" onClick={() => setCollapsed(true)} aria-label="收起到屏幕边缘"><FiChevronRight /></button></div></div>
        {searchOpen && <label className="global-search"><FiSearch /><input autoFocus value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索所有软件" /><button onClick={() => { setQuery(""); setSearchOpen(false); }} aria-label="关闭搜索"><FiX /></button></label>}
        <div className="category-list">{categories.map(({ id, name, count, color, icon: Icon }) => <button key={id} className={activeId === id ? "category-row active" : "category-row"} onClick={() => setActiveId(id)} style={{ "--accent": color }}><Icon /><span>{name}</span><small>{count}</small><FiChevronLeft className="open-arrow" /></button>)}</div>
        <button className="add-category" onClick={() => setAdding(true)}><FiPlus /><span>新增分类</span></button>
      </>}
    </aside>
    {notice && <div className="toast">{notice}</div>}
    {adding && <AddCategory onClose={() => setAdding(false)} onAdd={addCategory} />}
  </main>;
}

