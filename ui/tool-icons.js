const paths={
 home:'<path d="m3 10 9-8 9 8v10H3zM9 20v-8h6v8"/>',film:'<path d="M3 9h18v12H3zM3 8l17-5-1-3L2 5zm5-5 3 3m3-5 3 3"/>',code:'<path d="m8 7-5 5 5 5m8-10 5 5-5 5m-3-14-2 18"/>',play:'<path d="m8 4 12 8-12 8z"/>',chat:'<path d="M3 4h18v13h-9l-5 4v-4H3z"/>',music:'<path d="M9 17V5l11-2v12m-11 2a3 3 0 1 1-3-3h3m11 1a3 3 0 1 1-3-3h3"/>',
 history:'<path d="M3 10a9 9 0 1 1 2 8M3 4v6h6m3-4v6l4 2"/>',
 text:'<path d="M4 6V4h16v2m-8-1v15m-4 0h8"/>',link:'<path d="m10 14 4-4m-5 6-1 1a4 4 0 0 1-6-6l4-4a4 4 0 0 1 6 0m0 10a4 4 0 0 0 6 0l4-4a4 4 0 0 0-6-6l-1 1"/>',
 image:'<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="8" cy="9" r="1"/><path d="m4 17 5-5 4 4 3-3 4 4"/>',file:'<path d="M6 3h8l4 4v14H6zM13 3v5h5"/>',
 search:'<circle cx="10" cy="10" r="6"/><path d="m15 15 5 5"/>',gear:'<path d="m9 4 1-2h4l1 2 3 2 2 1 1 4-2 2-1 3v2l-3 2-3-1-3 1-3-2v-3l-2-2-2-2 1-4 3-1z"/><circle cx="12" cy="11" r="3"/>',
 x:'<path d="m6 6 12 12M18 6 6 18"/>',trash:'<path d="M4 6h16M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7m4-7v7"/>',
 clipboard:'<rect x="5" y="5" width="14" height="16" rx="3"/><rect x="9" y="3" width="6" height="4" rx="1"/>',
 phone:'<rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><path d="M11 18.5h2"/>',
 laptop:'<rect x="4.5" y="5" width="15" height="10" rx="1.5"/><path d="M2 19h20"/>',tablet:'<rect x="4.5" y="2.5" width="15" height="19" rx="2.5"/><path d="M11 18.5h2"/>',
 arrow:'<path d="M4 12h15m-6-6 6 6-6 6"/>',down:'<path d="m7 10 5 5 5-5"/>',right:'<path d="m9 6 6 6-6 6"/>',back:'<path d="m15 6-6 6 6 6"/>',
 up:'<path d="m7 14 5-5 5 5"/>',pin:'<path d="M9 4h6l-1 6 3 3H7l3-3zM12 13v7"/>',check:'<path d="m5 12 5 5 9-10"/>',edit:'<path d="M4 20h4L19 9l-4-4L4 16zM13 7l4 4"/>',copy:'<rect x="8" y="8" width="12" height="12" rx="2.5"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>',
 open:'<path d="M14 4h6v6m0-6L10 14m0-9H5v15h14v-5"/>',folder:'<path d="M3 7V5h7l2 2h9v13H3z"/>',
 convert:'<path d="M3 8h16l-4-4m6 12H5l4 4"/>',scan:'<path d="M5 11a10 10 0 0 1 14 0M8 14a6 6 0 0 1 8 0m-5 3a2 2 0 0 1 2 0"/>',plus:'<path d="M12 5v14M5 12h14"/>',app:'<rect x="3" y="4" width="18" height="16" rx="3"/><path d="M3 9h18"/>'
};
export function glyph(name){const svg=document.createElementNS('http://www.w3.org/2000/svg','svg');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('aria-hidden','true');svg.setAttribute('fill','none');svg.setAttribute('stroke','currentColor');svg.setAttribute('stroke-width','1.65');svg.setAttribute('stroke-linecap','round');svg.setAttribute('stroke-linejoin','round');svg.innerHTML=paths[name]||paths.file;return svg}
export function paintIcons(root=document){root.querySelectorAll('[data-glyph]').forEach(e=>e.replaceChildren(glyph(e.dataset.glyph)))}
export const ICONS=['home','film','code','folder','music','chat','play','link','image','file','text','app','phone','clipboard','search','convert','scan','gear'];
export const iconFor=n=>({main:'home',streaming:'film',code:'code'}[String(n.label).toLowerCase()]||'folder');
