use crate::{err,AppResult,SendQueue};
use serde::Serialize;
use serde_json::{json,Value};
use std::{collections::BTreeMap, io::Read, net::{Ipv4Addr,SocketAddr,TcpListener,UdpSocket}, path::Path, sync::{Arc,Mutex}, time::{Duration,Instant}};
use reqwest::{Client,Response};
use rustls::{client::danger::{HandshakeSignatureValid,ServerCertVerified,ServerCertVerifier},pki_types::{CertificateDer,ServerName,UnixTime},DigitallySignedStruct,SignatureScheme};
use tauri::{Emitter,Manager};
use futures_util::TryStreamExt;

const PORT:u16=53317;
const GROUP:Ipv4Addr=Ipv4Addr::new(224,0,0,167);
#[derive(Clone,Serialize)]
pub struct Peer { id:String, alias:String, model:String, address:String, protocol:String, kind:String, #[serde(skip)] fingerprint:String }
pub struct Network { peers:Mutex<BTreeMap<String,Peer>>, busy:tokio::sync::Mutex<()>, cert:Vec<u8>, key:Vec<u8>, id:String }
fn identity(app:&tauri::AppHandle,port:u16)->Value{
    let id=app.state::<Network>().id.clone();
    json!({"alias":format!("Sora · {}",std::env::var("COMPUTERNAME").unwrap_or_else(|_|"PC".into())),"version":"2.1","deviceModel":"Sora","deviceType":"desktop","fingerprint":id,"port":port,"protocol":"http","download":false})
}
impl Network {pub fn new()->AppResult<Self>{
    let ident=rcgen::generate_simple_self_signed(vec!["Sora".to_owned()]).map_err(err)?;
    let cert=ident.cert.der().to_vec();let key=ident.key_pair.serialize_der();let id=fingerprint(&cert).to_uppercase();
    Ok(Self{peers:Mutex::default(),busy:tokio::sync::Mutex::default(),cert,key,id})
}
/// Sora's LocalSend identity, shared with the receiving side (lan_receive.rs): fingerprint, certificate, key.
pub fn id(&self)->&str{&self.id}
pub fn cert(&self)->&[u8]{&self.cert}
pub fn key(&self)->&[u8]{&self.key}
}
fn local(ip:Ipv4Addr)->bool{ip.is_private()||ip.is_link_local()||ip.is_loopback()||if_addrs::get_if_addrs().unwrap_or_default().iter().any(|i|match &i.addr{if_addrs::IfAddr::V4(a)=>!a.ip.is_loopback()&&(u32::from(a.ip)&u32::from(a.netmask))==(u32::from(ip)&u32::from(a.netmask)),_=>false})}
fn fingerprint(bytes:&[u8])->String{ring::digest::digest(&ring::digest::SHA256,bytes).as_ref().iter().map(|b|format!("{b:02x}")).collect()}
fn peer(ip:Ipv4Addr,info:&Value,default_port:u16,default_scheme:&str)->Option<Peer>{
    if !local(ip)||!info["version"].as_str()?.starts_with("2."){return None}
    let protocol=info["protocol"].as_str().unwrap_or(default_scheme);
    let port=u16::try_from(info["port"].as_u64().unwrap_or(default_port.into())).ok()?;
    if port==0||!["http","https"].contains(&protocol){return None}
    let fp=info["fingerprint"].as_str()?.replace(':',"").to_ascii_lowercase();
    if fp.is_empty()||fp.len()>160{return None}
    if protocol=="https" && (fp.len()!=64 || !fp.bytes().all(|c|c.is_ascii_hexdigit())){return None}
    Some(Peer{id:format!("{protocol}/{ip}:{port}/{fp}"),alias:info["alias"].as_str()?.chars().take(64).collect(),model:info["deviceModel"].as_str().unwrap_or("").chars().take(48).collect(),address:format!("{ip}:{port}"),protocol:protocol.into(),kind:(if info["deviceType"].as_str()==Some("mobile"){"phone"}else{"laptop"}).into(),fingerprint:fp})
}
fn remember(net:&Network,peers:&[Peer])->AppResult<()>{
    let mut saved=net.peers.lock().map_err(err)?;
    if saved.len()+peers.len()>192{saved.clear()}
    for p in peers{saved.insert(p.id.clone(),p.clone());}Ok(())
}
fn discover(mut me:Value,interfaces:Vec<Ipv4Addr>,multicast_port:u16)->AppResult<Vec<Peer>>{
    let listener=TcpListener::bind((Ipv4Addr::UNSPECIFIED,0)).map_err(err)?;
    let port=listener.local_addr().map_err(err)?.port();
    let server=tiny_http::Server::from_listener(listener,None).map_err(err)?;
    me["port"]=port.into();me["announce"]=true.into();
    let packet=serde_json::to_vec(&me).map_err(err)?;
    let mut reply=me.clone();reply.as_object_mut().unwrap().remove("announce");
    let mut udp=None;
    let multicast=socket2::Socket::new(socket2::Domain::IPV4,socket2::Type::DGRAM,None).map_err(err)?;
    multicast.set_reuse_address(true).map_err(err)?;
    if multicast.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED,multicast_port)).into()).is_ok(){
        multicast.set_nonblocking(true).map_err(err)?;udp=Some(UdpSocket::from(multicast));
    }
    let mut sent=false;
    for ip in interfaces{
        if let Some(s)=&udp{let _=s.join_multicast_v4(&GROUP,&ip);}
        if let Ok(s)=UdpSocket::bind((ip,0)){let s=socket2::Socket::from(s);let _=s.set_multicast_if_v4(&ip);let _=s.set_multicast_loop_v4(ip.is_loopback());
            sent|=s.send_to(&packet,&SocketAddr::from((GROUP,multicast_port)).into()).is_ok();}
    }
    if !sent{return Err("No active LAN interface. Connect to Wi-Fi or enter the phone IP.".into())}
    let mut devices=BTreeMap::new();let start=Instant::now();let mut buf=[0;8192];
    while start.elapsed()<Duration::from_millis(2400)&&devices.len()<64{
        if let Some(s)=&udp{for _ in 0..16{let Ok((n,SocketAddr::V4(addr)))=s.recv_from(&mut buf)else{break};
            if let Ok(v)=serde_json::from_slice::<Value>(&buf[..n]){if v["fingerprint"]!=me["fingerprint"]{if let Some(p)=peer(*addr.ip(),&v,PORT,"https"){devices.insert(p.id.clone(),p);}}}
        }}
        if let Ok(Some(mut req))=server.recv_timeout(Duration::from_millis(30)){
            let valid=req.method()==&tiny_http::Method::Post && req.url()=="/api/localsend/v2/register" && req.body_length().is_some_and(|n|n<=8192);
            if valid{let addr=req.remote_addr().copied();let mut b=Vec::new();
                if req.as_reader().take(8193).read_to_end(&mut b).is_ok(){if let (Some(SocketAddr::V4(a)),Ok(v))=(addr,serde_json::from_slice::<Value>(&b)){
                    if v["fingerprint"]!=me["fingerprint"]{if let Some(p)=peer(*a.ip(),&v,PORT,"https"){devices.insert(p.id.clone(),p);}}
                }}
                let res=tiny_http::Response::from_string(reply.to_string()).with_header(tiny_http::Header::from_bytes("Content-Type","application/json").unwrap());let _=req.respond(res);
            }else{let _=req.respond(tiny_http::Response::empty(404));}
        }
    }
    Ok(devices.into_values().collect())
}
#[tauri::command]
pub async fn send_scan(app:tauri::AppHandle,method:Option<String>)->AppResult<Vec<Peer>>{tauri::async_runtime::spawn_blocking(move||{
    if method.as_deref()==Some("quickshare"){
        let peers:Vec<Peer>=crate::quick_share::discover(Duration::from_secs(6))?.into_iter().map(|f|Peer{id:format!("quickshare/{}/{}",f.addr,f.endpoint),alias:f.name.chars().take(64).collect(),model:f.host.clone().filter(|_|f.name.starts_with("Android ")).unwrap_or_default(),address:f.addr.to_string(),protocol:"quickshare".into(),kind:f.kind.into(),fingerprint:f.endpoint}).collect();
        remember(&app.state::<Network>(),&peers)?;return Ok(peers)
    }
    let interfaces=if_addrs::get_if_addrs().map_err(err)?.into_iter().filter_map(|i|match i.addr{if_addrs::IfAddr::V4(a) if !a.ip.is_loopback()=>Some(a.ip),_=>None}).collect();
    let peers=discover(identity(&app,PORT),interfaces,PORT)?;remember(&app.state::<Network>(),&peers)?;Ok(peers)
}).await.map_err(err)?}

#[derive(Debug)]
struct Trust(Option<String>);
impl ServerCertVerifier for Trust{
    fn verify_server_cert(&self,cert:&CertificateDer<'_>,_:&[CertificateDer<'_>],_:&ServerName<'_>,_:&[u8],_:UnixTime)->Result<ServerCertVerified,rustls::Error>{
        if self.0.as_ref().is_some_and(|fp|*fp!=fingerprint(cert.as_ref())){return Err(rustls::Error::General("Phone certificate changed. Discover it again.".into()))}
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self,msg:&[u8],cert:&CertificateDer<'_>,sig:&DigitallySignedStruct)->Result<HandshakeSignatureValid,rustls::Error>{
        rustls::crypto::verify_tls12_signature(msg,cert,sig,&rustls::crypto::ring::default_provider().signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self,msg:&[u8],cert:&CertificateDer<'_>,sig:&DigitallySignedStruct)->Result<HandshakeSignatureValid,rustls::Error>{
        rustls::crypto::verify_tls13_signature(msg,cert,sig,&rustls::crypto::ring::default_provider().signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self)->Vec<SignatureScheme>{rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()}
}
fn client(net:&Network,pin:Option<String>)->AppResult<Client>{
    let cfg=rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider())).with_safe_default_protocol_versions().map_err(err)?
        .dangerous().with_custom_certificate_verifier(Arc::new(Trust(pin)))
        .with_client_auth_cert(vec![CertificateDer::from(net.cert.clone())],rustls::pki_types::PrivatePkcs8KeyDer::from(net.key.clone()).into()).map_err(err)?;
    Client::builder().use_preconfigured_tls(cfg).tls_info(true).no_proxy().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(4)).build().map_err(err)
}
async fn json_body(mut r:Response)->AppResult<Value>{let mut b=Vec::new();while let Some(c)=r.chunk().await.map_err(err)?{if b.len()+c.len()>32768{return Err("Invalid device response".into())}b.extend(c);}serde_json::from_slice(&b).map_err(err)}
#[tauri::command]
pub async fn send_lookup(app:tauri::AppHandle,address:String)->AppResult<Peer>{
    let addr=address.trim();let addr=if addr.contains(':'){addr.to_owned()}else{format!("{addr}:{PORT}")};
    let a:std::net::SocketAddrV4=addr.parse().map_err(|_|"Enter a local IPv4 address, optionally with a port.")?;
    if !local(*a.ip())||a.port()==0{return Err("Use the phone’s local network address.".into())}
    let c=client(&app.state::<Network>(),None)?;
    for scheme in ["https","http"]{
        let Ok(r)=c.get(format!("{scheme}://{a}/api/localsend/v2/info")).timeout(Duration::from_secs(4)).send().await else{continue};
        if !r.status().is_success(){continue}
        let cert=r.extensions().get::<reqwest::tls::TlsInfo>().and_then(|i|i.peer_certificate()).map(fingerprint);
        let mut info=json_body(r).await?;info["port"]=a.port().into();info["protocol"]=scheme.into();
        let p=peer(*a.ip(),&info,a.port(),scheme).ok_or("This device did not identify itself as LocalSend v2.")?;
        if scheme=="https" && cert.as_deref()!=Some(&p.fingerprint){return Err("Device certificate does not match its identity.".into())}
        remember(&app.state::<Network>(),std::slice::from_ref(&p))?;return Ok(p)
    }
    Err("Phone unreachable · open LocalSend".into())
}
fn status(code:reqwest::StatusCode)->String{match code.as_u16(){
    401=>"PIN required or incorrect. Enter the phone’s PIN and retry.".into(),403=>"Transfer declined on the phone.".into(),409=>"Phone is busy with another transfer.".into(),429=>"Too many requests. Try again later.".into(),_=>format!("Phone returned HTTP {code}.")
}}
struct Outbound {
    id:String, path:String, name:String, size:u64, file:tokio::fs::File,
}
#[derive(Serialize)]
pub struct Delivery { completed:Vec<String>, skipped:usize, error:Option<String> }

fn file_name(source:&Path,used:&mut std::collections::HashSet<String>)->AppResult<String>{
    let original=source.file_name().ok_or("Missing filename")?.to_string_lossy();
    let mut candidate=original.to_string();
    let stem=source.file_stem().unwrap_or_default().to_string_lossy();
    let ext=source.extension().map(|s|format!(".{}",s.to_string_lossy())).unwrap_or_default();
    for n in 2..{
        if used.insert(candidate.to_lowercase()){return Ok(candidate)}
        candidate=format!("{stem} ({n}){ext}");
    }
    unreachable!()
}
#[tauri::command]
pub async fn send_files(app:tauri::AppHandle,window:tauri::WebviewWindow,device:String,paths:Vec<String>,pin:String)->AppResult<Delivery>{
    let net=app.state::<Network>();let _job=net.busy.try_lock().map_err(|_|"A transfer is already running.")?;
    let peer=net.peers.lock().map_err(err)?.get(&device).cloned().ok_or("Select a nearby device.")?;
    if paths.is_empty()||paths.len()>80{return Err("Choose 1–80 files to send.".into())}
    {
        let queue=app.state::<SendQueue>();let queue=queue.0.lock().map_err(err)?;
        if paths.iter().any(|path|!queue.iter().any(|s|s.path==*path&&!s.directory)){return Err("A selected file is no longer in Send.".into())}
    }
    let mut seen=std::collections::HashSet::new();let mut names=std::collections::HashSet::new();
    if peer.protocol=="quickshare"{return quick_share(window,&peer,paths).await}
    let mut batch=Vec::new();let mut manifest=serde_json::Map::new();
    for path in paths{
        if !seen.insert(path.clone()){continue}
        let name=file_name(Path::new(&path),&mut names)?;
        let file=tokio::fs::File::open(&path).await.map_err(|_|format!("Cannot open {name}."))?;
        let meta=file.metadata().await.map_err(err)?;if !meta.is_file(){return Err(format!("{name} is not a regular file."))}
        let id=uuid::Uuid::new_v4().to_string();let size=meta.len();
        manifest.insert(id.clone(),json!({"id":id,"fileName":name,"size":size,"fileType":"application/octet-stream"}));
        batch.push(Outbound{id,path,name,size,file});
    }
    let base=format!("{}://{}/api/localsend/v2",peer.protocol,peer.address);
    let c=client(&net,Some(peer.fingerprint))?;
    let mut sender=identity(&app,PORT);sender["protocol"]=peer.protocol.into();
    let mut prep=c.post(format!("{base}/prepare-upload")).json(&json!({"info":sender,"files":manifest})).timeout(Duration::from_secs(90));
    if !pin.is_empty(){prep=prep.query(&[("pin",&pin)]);}
    let response=prep.send().await.map_err(|_|"Could not reach or verify the selected device.")?;
    if response.status().as_u16()==204{return Ok(Delivery{completed:batch.into_iter().map(|f|f.path).collect(),skipped:0,error:None})}
    if !response.status().is_success(){return Err(status(response.status()))}
    let info=json_body(response).await?;let session=info["sessionId"].as_str().ok_or("Missing transfer session.")?;
    let count=batch.len();batch.retain(|f|info["files"][&f.id].as_str().is_some_and(|s|!s.is_empty()));
    let accepted=batch.len();let total=batch.iter().map(|f|f.size).sum::<u64>();
    let mut result=Delivery{completed:Vec::new(),skipped:count-accepted,error:None};
    let mut finished=0u64;
    for (i,f) in batch.into_iter().enumerate(){
        let token=info["files"][&f.id].as_str().unwrap();let mut bytes=0u64;let size=f.size;let name=f.name.clone();
        let progress=window.clone();let mut last=Instant::now();
        let _=window.emit("send-progress",json!({"index":i+1,"files":accepted,"name":name,"sent":finished,"total":total}));
        let stream=tokio_util::io::ReaderStream::with_capacity(f.file,65536).inspect_ok(move|chunk|{
            bytes+=chunk.len() as u64;if last.elapsed()>Duration::from_millis(130)||bytes==size{
                let _=progress.emit("send-progress",json!({"index":i+1,"files":accepted,"name":name,"sent":finished+bytes,"total":total}));last=Instant::now();
            }
        });
        let sent=c.post(format!("{base}/upload")).query(&[("sessionId",session),("fileId",&f.id),("token",token)])
            .header("Content-Type","application/octet-stream").header("Content-Length",size)
            .timeout(Duration::from_secs(3600)).body(reqwest::Body::wrap_stream(stream)).send().await;
        match sent{Ok(r) if r.status().is_success()=>{finished+=size;result.completed.push(f.path);continue},
            Ok(r)=>result.error=Some(status(r.status())),Err(_)=>result.error=Some(format!("Connection lost during {}.",f.name))}
        let _=c.post(format!("{base}/cancel")).query(&[("sessionId",session)]).timeout(Duration::from_secs(2)).send().await;break
    }
    Ok(result)
}
/// The same batch through Quick Share (see quick_share.rs); the phone's user accepts or declines all files at once.
async fn quick_share(window:tauri::WebviewWindow,peer:&Peer,paths:Vec<String>)->AppResult<Delivery>{
    let mut seen=std::collections::HashSet::new();let mut names=std::collections::HashSet::new();let mut items=Vec::new();
    for path in paths{
        if !seen.insert(path.clone()){continue}
        let name=file_name(Path::new(&path),&mut names)?;
        let meta=std::fs::metadata(&path).map_err(|_|format!("Cannot open {name}."))?;if !meta.is_file(){return Err(format!("{name} is not a regular file."))}
        items.push(crate::quick_share::Item{path,name,size:meta.len()});
    }
    let addr:SocketAddr=peer.address.parse().map_err(err)?;
    let (count,me)=(items.len(),format!("Sora · {}",std::env::var("COMPUTERNAME").unwrap_or_else(|_|"PC".into())));
    let out=tauri::async_runtime::spawn_blocking(move||crate::quick_share::send(addr,&me,items,|e|match e{
        crate::quick_share::Event::Code(code)=>{let _=window.emit("send-code",code);}
        crate::quick_share::Event::Progress{index,name,sent,total}=>{let _=window.emit("send-progress",json!({"index":index,"files":count,"name":name,"sent":sent,"total":total}));}
    })).await.map_err(err)??;
    Ok(Delivery{skipped:if out.declined{count}else{0},completed:out.completed,error:out.error})
}
#[tauri::command]
pub async fn send_file(app:tauri::AppHandle,window:tauri::WebviewWindow,device:String,path:String,pin:String)->AppResult<()>{
    let r=send_files(app,window,device,vec![path],pin).await?;
    if let Some(e)=r.error{return Err(e)}if r.skipped>0{return Err("Phone declined this file.".into())}Ok(())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn discovery_over_loopback(){
        use std::io::Write;
        let ip=Ipv4Addr::LOCALHOST;
        let s=socket2::Socket::new(socket2::Domain::IPV4,socket2::Type::DGRAM,None).unwrap();
        s.set_reuse_address(true).unwrap();s.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED,0)).into()).unwrap();
        let recv=UdpSocket::from(s);let port=recv.local_addr().unwrap().port();
        recv.join_multicast_v4(&GROUP,&ip).unwrap();recv.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
        let phone=std::thread::spawn(move||{
            let mut bytes=[0;2048];let (n,_)=recv.recv_from(&mut bytes).unwrap();
            let ad:Value=serde_json::from_slice(&bytes[..n]).unwrap();assert_eq!(ad["announce"],true);
            let body=r#"{"alias":"Loopback phone","version":"2.1","protocol":"http","port":53317,"fingerprint":"phone-id"}"#;
            let mut tcp=std::net::TcpStream::connect((ip,ad["port"].as_u64().unwrap() as u16)).unwrap();
            write!(tcp,"POST /api/localsend/v2/register HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            let mut response=String::new();tcp.read_to_string(&mut response).unwrap();assert!(response.starts_with("HTTP/1.1 200"));
        });
        let list=discover(json!({"fingerprint":"test-sora","version":"2.1","protocol":"http"}),vec![ip],port).unwrap();phone.join().unwrap();
        assert_eq!(list.len(),1);assert_eq!(list[0].alias,"Loopback phone");assert_eq!(list[0].address,"127.0.0.1:53317");
    }
}
