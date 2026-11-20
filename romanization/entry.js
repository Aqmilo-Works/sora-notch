import { pinyin } from 'pinyin-pro';
import { romanize } from 'es-hangul';
export const hasCjk = text => /[\u3040-\u30ff\u3400-\u9fff\uac00-\ud7af]/u.test(text);
function script(src){return new Promise((resolve,reject)=>{const s=document.createElement('script');s.src=src;s.onload=resolve;s.onerror=()=>reject(Error('The romanization dictionary could not be loaded'));document.head.append(s)})}
let japanese;
async function ja(){if(!japanese)japanese=(async()=>{await script('vendor/kuroshiro.min.js');await script('vendor/kuroshiro-analyzer-kuromoji.min.js');const Constructor=window.Kuroshiro.default||window.Kuroshiro;const Analyzer=window.KuromojiAnalyzer.default||window.KuromojiAnalyzer;const k=new Constructor();await k.init(new Analyzer({dictPath:'vendor/dict/'}));return k})();return japanese}
const KANA=/[\u3040-\u30ff]/u,HANGUL=/[\uac00-\ud7af]/u,HANGUL_RUN=/([\uac00-\ud7af]+(?:[ \t]+[\uac00-\ud7af]+)*)/u,HAN=/[\u3400-\u9fff]/u;
// Pick the main language from the lyrics themselves (not the title), so a Korean song with one
// Japanese phrase is still Korean. Mixed lines are then converted segment by segment.
function detect(all,context){
 const count=re=>(all.match(new RegExp(re.source,'gu'))||[]).length;
 const kana=count(KANA),hangul=count(HANGUL);
 if(hangul&&hangul>=kana)return 'ko';
 if(kana)return 'ja';
 return KANA.test(context)?'ja':HANGUL.test(context)?'ko':'zh';
}
export async function romanizeLines(lines,context='',language='auto'){
 const all=lines.map(l=>l.text).join('\n');
 const lang=language==='auto'?detect(all,context):language;
 const result=[];
 for(const line of lines){
  let text=line.text;
  if(hasCjk(text)){
   const parts=text.split(HANGUL_RUN),out=[];
   for(const part of parts){
    if(!part)continue;
    if(HANGUL.test(part))out.push(romanize(part));
    else if(!hasCjk(part))out.push(part);
    else if(KANA.test(part)||lang==='ja')out.push(await (await ja()).convert(part,{to:'romaji',mode:'spaced',romajiSystem:'hepburn'}));
    else if(HAN.test(part)&&lang!=='ko')out.push(pinyin(part,{toneType:'symbol',nonZh:'consecutive'}));
    else out.push(part);
   }
   text=out.join('').replace(/ +/g,' ').replace(/([([]) /g,'$1').replace(/ ([,.!?)]|])/g,'$1').trim();
  }
  result.push({...line,text});
 }
 return {lines:result,language:lang};
}
