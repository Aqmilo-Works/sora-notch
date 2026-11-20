// Native (Windows-style) hover tooltips clash with the notch, so pages have none: every title attribute, including ones set later
// by code, becomes an aria-label (screen readers still get it when the element has no text of its own) and is removed.
const strip=el=>{const t=el.getAttribute('title');if(t===null)return;if(!el.hasAttribute('aria-label')&&!el.textContent.trim())el.setAttribute('aria-label',t);el.removeAttribute('title')};
const sweep=node=>{if(node.nodeType!==1)return;strip(node);node.querySelectorAll('[title]').forEach(strip)};
sweep(document.documentElement);
new MutationObserver(list=>{for(const m of list){if(m.type==='attributes')strip(m.target);else m.addedNodes.forEach(sweep)}})
 .observe(document.documentElement,{subtree:true,childList:true,attributes:true,attributeFilter:['title']});
