const $ = s => document.querySelector(s);
const $$ = s => [...document.querySelectorAll(s)];

let supervisorMode = "manual";
let running = false;
let currentProfile = "cheap";
let appConfig=null;
let editingProfile=null;

const roleFields={cheap:"cheap_model_id",premium:"advanced_model_id",judge:"local_supervisor_model_id"};
const selectIds={cheap:"cheapSelect",premium:"premiumSelect",judge:"judgeSelect"};

function uuid(){ return crypto.randomUUID?crypto.randomUUID():`${Date.now()}-0000-4000-8000-${Math.random().toString(16).slice(2,14)}`; }

function renderModelSelects(){
  if(!appConfig) return;
  for(const kind of Object.keys(selectIds)){
    const select=$("#"+selectIds[kind]);
    const selected=appConfig.roles[roleFields[kind]]||"";
    select.innerHTML=appConfig.profiles.map(profile=>`<option value="${profile.id}">${escapeHtml(profile.display_name)}</option>`).join("")+
      `<option value="add">＋ 添加新模型</option>`;
    select.value=selected&&appConfig.profiles.some(profile=>profile.id===selected)?selected:"add";
  }
}

async function loadAppConfig(){
  const response=await fetch("/api/config");
  if(!response.ok) throw new Error("无法读取配置");
  appConfig=await response.json();
  supervisorMode=appConfig.supervisor_mode==="local_ai"?"local":"manual";
  $("#judgeMarkdown").value=appConfig.local_ai_supervisor?.markdown||"";
  $("#listenHost").value="127.0.0.1";
  $("#listenPort").value=appConfig.server.port;
  $("#virtualModel").value=appConfig.server.virtual_model;
  renderModelSelects();
  updateModeUI();
}

async function saveAppConfig(){
  const response=await fetch("/api/config",{method:"PUT",headers:{"Content-Type":"application/json"},body:JSON.stringify(appConfig)});
  if(!response.ok){ const data=await response.json().catch(()=>null); throw new Error(data?.error?.message||"配置保存失败"); }
}

function toast(msg){
  const t=$("#toast");
  t.textContent=msg;
  t.classList.add("show");
  setTimeout(()=>t.classList.remove("show"),1500);
}

function openOverlay(id){ $("#"+id).classList.add("show"); }
function closeOverlay(id){ $("#"+id).classList.remove("show"); }

$$("[data-close]").forEach(btn=>{
  btn.addEventListener("click",()=>closeOverlay(btn.dataset.close));
});
$$(".overlay").forEach(o=>{
  o.addEventListener("click",e=>{if(e.target===o)o.classList.remove("show")});
});

$$(".nav-btn").forEach(btn=>{
  btn.addEventListener("click",()=>{
    $$(".nav-btn").forEach(x=>x.classList.remove("active"));
    $$(".page").forEach(x=>x.classList.remove("active"));
    btn.classList.add("active");
    $("#"+btn.dataset.page).classList.add("active");
  });
});

// GUI 依赖 Axum，因此这里仅启停 Gateway，不关闭管理服务。
async function setGatewayState(enabled){
  const response=await fetch(`/api/gateway/${enabled?"start":"stop"}`,{method:"POST"});
  if(!response.ok){
    const error=await response.json().catch(()=>null);
    throw new Error(error?.error?.message || `请求失败（${response.status}）`);
  }
  running=enabled;
  const host=$("#listenHost").value || "127.0.0.1";
  const port=$("#listenPort").value || "8181";
  const vmodel=$("#virtualModel").value || "modelgate-auto";
  const base=`http://${host}:${port}/v1`;

  $("#runBtn").textContent=running?"■":"▶";
  $("#runBtn").title=running?"停止运行":"开始运行";
  $("#statusDot").classList.toggle("on",running);
  $("#statusText").textContent=running?"正在运行":"未运行";
  $("#statusSub").textContent=running?"本地 OpenAI 兼容接口已启用":"点击右下角 ▶ 启用 Gateway";
  $("#apiPanel").classList.toggle("show",running);

  $("#localBase").textContent=base;
  $("#localModel").textContent=vmodel;
  $("#localChat").textContent=base+"/chat/completions";
  $("#localModels").textContent=base+"/models";

  toast(running?"ModelGate Gateway 已启用":"ModelGate Gateway 已停用");
}

$("#runBtn").addEventListener("click",async()=>{
  try{ await setGatewayState(!running); }
  catch(error){ toast(error.message); }
});

fetch("/api/status")
  .then(response=>response.json())
  .then(status=>{ if(status.gateway_enabled) return setGatewayState(true); })
  .catch(()=>toast("无法读取 Gateway 状态"));

$$("[data-copy]").forEach(btn=>{
  btn.addEventListener("click",async()=>{
    const text=$("#"+btn.dataset.copy).textContent;
    try{ await navigator.clipboard.writeText(text); }catch(e){}
    toast("已复制："+text);
  });
});

async function loadProfile(kind){
  if(!appConfig) await loadAppConfig();
  currentProfile=kind;
  const selectedId=appConfig.roles[roleFields[kind]];
  editingProfile=appConfig.profiles.find(profile=>profile.id===selectedId)||{
    id:uuid(),display_name:"",model_name:"",base_url:"",api_key:""
  };
  $("#modelModalTitle").textContent=
    kind==="cheap"?"低价模型设置":
    kind==="premium"?"专业模型设置":"本地监管模型设置";
  $("#mDisplay").value=editingProfile.display_name;
  $("#mName").value=editingProfile.model_name;
  $("#mBase").value=editingProfile.base_url;
  $("#mKey").value="";
  $("#mKey").type="password";
  $("#toggleKey").textContent="显示";
  $("#modelTestResult").textContent="";
  openOverlay("modelOverlay");
}
$$(".gear[data-profile]").forEach(btn=>{
  btn.addEventListener("click",()=>loadProfile(btn.dataset.profile));
});
$("#judgeGear").addEventListener("click",()=>{
  // 本地监管模型的齿轮位于 localOverlay 内。
  // 如果直接再打开 modelOverlay，两层弹窗会因为层级相同而互相遮挡。
  // 因此这里先暂时关闭本地 AI 设置层，再打开模型设置。
  closeOverlay("localOverlay");
  loadProfile("judge");
});

function closeModelEditor(){
  closeOverlay("modelOverlay");
  if(currentProfile==="judge"){
    openOverlay("localOverlay");
  }
}

$("#closeModelOverlay").addEventListener("click", closeModelEditor);
$("#cancelModelOverlay").addEventListener("click", closeModelEditor);


$("#toggleKey").addEventListener("click",()=>{
  const input=$("#mKey");
  const show=input.type==="password";
  input.type=show?"text":"password";
  $("#toggleKey").textContent=show?"隐藏":"显示";
});
$("#testModelBtn").addEventListener("click",async()=>{
  $("#modelTestResult").textContent="正在测试连接……";
  $("#modelTestResult").className="test-result";
  try{
    const draft={id:editingProfile.id,display_name:$("#mDisplay").value,model_name:$("#mName").value,base_url:$("#mBase").value,api_key:$("#mKey").value};
    const response=await fetch("/api/models/test",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(draft)});
    if(!response.ok){ const data=await response.json().catch(()=>null); throw new Error(data?.error?.message||"连接失败"); }
    $("#modelTestResult").textContent="✓ 连接成功";
    $("#modelTestResult").className="test-result success";
  }catch(error){ $("#modelTestResult").textContent=error.message; }
});
$("#saveModelBtn").addEventListener("click",async()=>{
  const saved={id:editingProfile.id,display_name:$("#mDisplay").value.trim(),model_name:$("#mName").value.trim(),base_url:$("#mBase").value.trim(),api_key:$("#mKey").value};
  if(!saved.display_name||!saved.model_name||!saved.base_url){ toast("请完整填写模型配置"); return; }
  const index=appConfig.profiles.findIndex(profile=>profile.id===saved.id);
  if(index>=0) appConfig.profiles[index]=saved; else appConfig.profiles.push(saved);
  appConfig.roles[roleFields[currentProfile]]=saved.id;
  try{ await saveAppConfig(); }catch(error){ toast(error.message); return; }
  renderModelSelects();
  closeOverlay("modelOverlay");
  toast("模型配置已保存");

  // 如果刚刚编辑的是本地监管模型，保存后回到监管设置页，
  // 这样不会让用户突然掉回主界面。
  if(currentProfile==="judge"){
    openOverlay("localOverlay");
  }
});

["cheapSelect","premiumSelect","judgeSelect"].forEach(id=>{
  $("#"+id).addEventListener("change",e=>{
    if(e.target.value==="add"){
      loadProfile(id==="cheapSelect"?"cheap":id==="premiumSelect"?"premium":"judge");
    }else if(appConfig){
      const kind=id==="cheapSelect"?"cheap":id==="premiumSelect"?"premium":"judge";
      appConfig.roles[roleFields[kind]]=e.target.value;
      saveAppConfig().catch(error=>toast(error.message));
    }
  });
});

loadAppConfig().catch(error=>toast(error.message));

$$("[data-settings-modal]").forEach(btn=>{
  btn.addEventListener("click",()=>{
    const type=btn.dataset.settingsModal;
    if(type==="mode") openOverlay("modeOverlay");
    if(type==="manual") {
      updateModeUI();
      openOverlay("manualOverlay");
    }
    if(type==="local") {
      updateModeUI();
      openOverlay("localOverlay");
    }
    if(type==="server") openOverlay("serverOverlay");
  });
});
$("#aboutBtn").addEventListener("click",()=>openOverlay("aboutOverlay"));

async function setMode(mode){
  supervisorMode=mode;
  if(appConfig){
    appConfig.supervisor_mode=mode==="local"?"local_ai":"visual_rules";
    try{ await saveAppConfig(); }catch(error){ toast(error.message); return; }
  }
  updateModeUI();
  toast(mode==="manual"?"已切换到可视化规则监管":"已切换到本地蒸馏 AI 监管");
}
function updateModeUI(){
  const local=supervisorMode==="local";
  $("#manualModeBtn").classList.toggle("active",!local);
  $("#localModeBtn").classList.toggle("active",local);
  $("#modeSummary").textContent="当前："+(local?"本地蒸馏 AI 监管":"可视化规则监管");
  $("#modeExplain").textContent=local
    ?"当前由本地蒸馏 AI 按 Markdown 监管规则判断低价模型是否异常；可视化规则监管暂停使用。"
    :"当前使用可视化规则监管低价模型输出；本地监管 AI 不参与判断。";
  $("#manualEditor").classList.toggle("disabled",local);
  $("#manualDisabledNote").style.display=local?"block":"none";
  $("#localDisabledNote").textContent=local
    ?"本地 AI 监管已启用。可视化规则暂时失效。"
    :"当前未启用本地 AI 监管。你仍然可以预先编辑配置，启用后才会生效。";
}
$("#manualModeBtn").addEventListener("click",()=>setMode("manual"));
$("#localModeBtn").addEventListener("click",()=>setMode("local"));

// 多规则编辑器使用带类型的嵌套积木；UI 节点会在保存前转换成后端 AST。

let typedRules = [];
let typedDrag = null;

const NODE_DEFS = {
  "event.standard_completed": {out:"event", label:"当低价模型完成回答"},
  "event.advanced_completed": {out:"event", label:"当高级模型完成回答"},
  "event.standard_finished": {out:"event", label:"当低价模型请求结束"},
  "event.advanced_finished": {out:"event", label:"当高级模型请求结束"},

  "text.reasoning": {out:"text", label:"模型思考内容"},
  "text.output": {out:"text", label:"最终回答"},
  "text.user": {out:"text", label:"用户当前消息"},
  "text.system": {out:"text", label:"系统提示词"},
  "text.literal": {out:"text", label:"文本常量", editable:true, default:""},

  "number.reasoning_tokens": {out:"number", label:"思考 Token 数"},
  "number.output_tokens": {out:"number", label:"输出 Token 数"},
  "number.latency": {out:"number", label:"响应时间 ms"},
  "number.http_status": {out:"number", label:"HTTP 状态码"},
  "number.literal": {out:"number", label:"数字常量", editable:true, default:"5000"},

  "pattern.literal": {out:"pattern", label:"正则表达式", editable:true, default:"(?i)error"},

  "bool.contains": {out:"boolean", label:"包含", inputs:[["left","text"],["right","text"]]},
  "bool.not_contains": {out:"boolean", label:"不包含", inputs:[["left","text"],["right","text"]]},
  "bool.equals_text": {out:"boolean", label:"等于", inputs:[["left","text"],["right","text"]]},
  "bool.regex": {out:"boolean", label:"匹配正则", inputs:[["text","text"],["pattern","pattern"]]},
  "bool.gt": {out:"boolean", label:"大于", inputs:[["left","number"],["right","number"]]},
  "bool.lt": {out:"boolean", label:"小于", inputs:[["left","number"],["right","number"]]},
  "bool.gte": {out:"boolean", label:"大于等于", inputs:[["left","number"],["right","number"]]},
  "bool.lte": {out:"boolean", label:"小于等于", inputs:[["left","number"],["right","number"]]},
  "bool.and": {out:"boolean", label:"并且", inputs:[["left","boolean"],["right","boolean"]]},
  "bool.or": {out:"boolean", label:"或者", inputs:[["left","boolean"],["right","boolean"]]},
  "bool.not": {out:"boolean", label:"不是", inputs:[["value","boolean"]]},

  "action.use_standard": {out:"action", label:"使用低价模型"},
  "action.use_advanced": {out:"action", label:"使用高级模型"},
  "action.retry": {out:"action", label:"重新请求当前模型"},
  "action.log": {out:"action", label:"仅记录日志"}
};

function newRule(name="新规则"){
  return {
    id: crypto.randomUUID ? crypto.randomUUID() : String(Date.now()+Math.random()),
    name,
    event:null,
    condition:null,
    actions:[]
  };
}

function newNode(type,preset={}){
  const def=NODE_DEFS[type];
  const node={
    id: crypto.randomUUID ? crypto.randomUUID() : String(Date.now()+Math.random()),
    type,
    out:def.out
  };
  if(def.editable) node.value=preset.value ?? def.default ?? "";
  if(def.inputs){
    node.inputs={};
    def.inputs.forEach(([name])=>node.inputs[name]=null);
  }
  return node;
}

function cloneNode(node){ return JSON.parse(JSON.stringify(node)); }

function findRule(ruleId){
  return typedRules.find(r=>r.id===ruleId) || null;
}

function getNodeById(root,id){
  if(!root) return null;
  if(root.id===id) return root;
  if(root.inputs){
    for(const child of Object.values(root.inputs)){
      const found=getNodeById(child,id);
      if(found) return found;
    }
  }
  return null;
}

function removeNodeById(root,id){
  if(!root) return root;
  if(root.id===id) return null;
  if(root.inputs){
    for(const key of Object.keys(root.inputs)){
      root.inputs[key]=removeNodeById(root.inputs[key],id);
    }
  }
  return root;
}

function getNodeAtPath(rule,path){
  if(path==="event") return rule.event;
  if(path==="condition") return rule.condition;
  const parts=path.split(".");
  let node=rule.condition;
  for(const part of parts){
    if(part==="condition") continue;
    if(!node?.inputs) return null;
    node=node.inputs[part];
  }
  return node;
}

function setNodeAtPath(rule,path,node){
  if(path==="event"){ rule.event=node; return; }
  if(path==="condition"){ rule.condition=node; return; }

  const parts=path.split(".");
  let cur=rule.condition;
  for(let i=1;i<parts.length-1;i++){
    cur=cur?.inputs?.[parts[i]];
    if(!cur) return;
  }
  if(cur?.inputs) cur.inputs[parts[parts.length-1]]=node;
}

function getTypeClass(out){
  return out==="text"?"type-text":
         out==="number"?"type-number":
         out==="pattern"?"type-pattern":
         out==="boolean"?"type-bool":
         out==="action"?"type-action":
         out==="event"?"type-event":"type-text";
}

function escapeHtml(s){
  return String(s).replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
}

function renderNode(node,ruleId,path){
  if(!node) return "";
  const def=NODE_DEFS[node.type];

  if(node.out==="event"){
    return `<span class="node-wrap">
      <span class="tblock event type-event" draggable="true"
        data-rule-id="${ruleId}" data-node-id="${node.id}" data-node-path="${path}">
        ${def.label}
      </span>
      <button class="node-remove" data-rule-id="${ruleId}" data-remove-node="${node.id}" title="删除">×</button>
    </span>`;
  }

  if(["text","number","pattern"].includes(node.out)){
    if(def.editable){
      return `<span class="node-wrap">
        <span class="tblock value ${getTypeClass(node.out)}" draggable="true"
          data-rule-id="${ruleId}" data-node-id="${node.id}" data-node-path="${path}">
          ${def.label}
          <input class="inline-input" data-rule-id="${ruleId}" data-edit-node="${node.id}" value="${escapeHtml(node.value??"")}">
        </span>
        <button class="node-remove" data-rule-id="${ruleId}" data-remove-node="${node.id}">×</button>
      </span>`;
    }
    return `<span class="node-wrap">
      <span class="tblock value ${getTypeClass(node.out)}" draggable="true"
        data-rule-id="${ruleId}" data-node-id="${node.id}" data-node-path="${path}">
        ${def.label}
      </span>
      <button class="node-remove" data-rule-id="${ruleId}" data-remove-node="${node.id}">×</button>
    </span>`;
  }

  if(node.out==="boolean"){
    if(node.type==="bool.not"){
      return `<span class="node-wrap">
        <span class="bool-group" draggable="true"
          data-rule-id="${ruleId}" data-node-id="${node.id}" data-node-path="${path}">
          <span class="bool-group-head">不是</span>
          <span class="logic-inner">${renderSlot(node.inputs.value,"boolean",ruleId,path+".value")}</span>
        </span>
        <button class="node-remove" data-rule-id="${ruleId}" data-remove-node="${node.id}">×</button>
      </span>`;
    }

    if(node.type==="bool.and" || node.type==="bool.or"){
      return `<span class="node-wrap">
        <span class="bool-group" draggable="true"
          data-rule-id="${ruleId}" data-node-id="${node.id}" data-node-path="${path}">
          <span class="bool-group-head">${def.label}</span>
          <span class="logic-inner">
            ${renderSlot(node.inputs.left,"boolean",ruleId,path+".left")}
            ${renderSlot(node.inputs.right,"boolean",ruleId,path+".right")}
          </span>
        </span>
        <button class="node-remove" data-rule-id="${ruleId}" data-remove-node="${node.id}">×</button>
      </span>`;
    }

    const [a,b]=def.inputs;
    return `<span class="node-wrap">
      <span class="tblock boolean type-bool" draggable="true"
        data-rule-id="${ruleId}" data-node-id="${node.id}" data-node-path="${path}">
        ${renderSlot(node.inputs[a[0]],a[1],ruleId,path+"."+a[0])}
        <span>${def.label}</span>
        ${renderSlot(node.inputs[b[0]],b[1],ruleId,path+"."+b[0])}
      </span>
      <button class="node-remove" data-rule-id="${ruleId}" data-remove-node="${node.id}">×</button>
    </span>`;
  }

  return "";
}

function renderSlot(child,accept,ruleId,path){
  if(child){
    return `<span class="slot ${accept}-slot" data-rule-id="${ruleId}" data-accept="${accept}" data-path="${path}">
      ${renderNode(child,ruleId,path)}
    </span>`;
  }

  if(["text","number","pattern"].includes(accept)){
    const placeholder=accept==="text"?"直接输入文本":accept==="number"?"直接输入数值":"直接输入正则";
    return `<span class="slot ${accept}-slot" data-rule-id="${ruleId}" data-accept="${accept}" data-path="${path}">
      <input class="slot-direct-input" data-rule-id="${ruleId}" data-direct-path="${path}" data-direct-type="${accept}" placeholder="${placeholder}">
      <span class="slot-drop-hint">或拖入积木</span>
    </span>`;
  }

  return `<span class="slot ${accept}-slot" data-rule-id="${ruleId}" data-accept="${accept}" data-path="${path}">
    <span class="slot-placeholder">${accept==="boolean"?"拖入条件":accept==="event"?"拖入事件":"拖入积木"}</span>
  </span>`;
}

function renderRules(){
  const list=$("#rulesList");
  list.innerHTML="";
  $("#emptyRules").style.display=typedRules.length?"none":"block";

  typedRules.forEach((rule,index)=>{
    const card=document.createElement("div");
    card.className="rule-program";
    card.innerHTML=`
      <div class="rule-program-head">
        <div class="rule-program-title">
          <span class="rule-index">${index+1}</span>
          <input class="rule-program-name" data-rule-name="${rule.id}" value="${escapeHtml(rule.name)}">
        </div>
        <button class="rule-delete" data-delete-rule="${rule.id}">删除这条规则</button>
      </div>

      <div class="rule-program-body">
        <div class="rule-root multi">
          <div class="event-cap">
            <span style="margin-right:8px">触发：</span>
            <div class="slot event-slot" data-rule-id="${rule.id}" data-accept="event" data-path="event">
              ${rule.event ? renderNode(rule.event,rule.id,"event") : '<span class="slot-placeholder">拖入触发事件</span>'}
            </div>
          </div>

          <div class="cblock">
            <div class="cblock-head">
              如果
              <div class="slot boolean-slot" data-rule-id="${rule.id}" data-accept="boolean" data-path="condition">
                ${rule.condition ? renderNode(rule.condition,rule.id,"condition") : '<span class="slot-placeholder">拖入条件积木</span>'}
              </div>
            </div>

            <div class="cblock-body">
              <div style="font-size:12px;color:var(--muted);margin-bottom:7px">那么：</div>
              <div class="action-stack" data-rule-action-stack="${rule.id}">
                ${rule.actions.map((a,i)=>`
                  <div class="node-wrap">
                    <div class="tblock action type-action" draggable="true"
                      data-rule-id="${rule.id}" data-action-index="${i}" data-node-id="${a.id}">
                      ${NODE_DEFS[a.type].label}
                    </div>
                    <button class="node-remove" data-rule-id="${rule.id}" data-remove-action="${i}">×</button>
                  </div>
                `).join("")}
              </div>
              <div class="action-dropzone" data-rule-id="${rule.id}" data-accept="action">拖入动作积木</div>
            </div>
          </div>
        </div>
      </div>
    `;
    list.appendChild(card);
  });

  bindRuleEditorEvents();
  bindTypedDnD();
  updateTypedJson();
}

function bindRuleEditorEvents(){
  $$("[data-rule-name]").forEach(input=>{
    input.oninput=()=>{
      const rule=findRule(input.dataset.ruleName);
      if(rule) rule.name=input.value;
      updateTypedJson();
    };
  });

  $$("[data-delete-rule]").forEach(btn=>{
    btn.onclick=()=>{
      beginDeleteRule(btn.dataset.deleteRule);
    };
  });

  $$("[data-direct-path]").forEach(input=>{
    input.addEventListener("mousedown",e=>e.stopPropagation());
    input.addEventListener("dragstart",e=>e.stopPropagation());

    function commit(){
      const rule=findRule(input.dataset.ruleId);
      if(!rule || input.value.trim()==="") return;
      const t=input.dataset.directType;
      const nodeType=t==="text"?"text.literal":t==="number"?"number.literal":"pattern.literal";
      setNodeAtPath(rule,input.dataset.directPath,newNode(nodeType,{value:input.value}));
      renderRules();
    }

    input.onchange=commit;
    input.onkeydown=e=>{
      if(e.key==="Enter"){e.preventDefault();commit();}
    };
  });

  $$("[data-edit-node]").forEach(input=>{
    input.addEventListener("mousedown",e=>e.stopPropagation());
    input.addEventListener("dragstart",e=>e.stopPropagation());
    input.oninput=()=>{
      const rule=findRule(input.dataset.ruleId);
      if(!rule) return;
      const node=getNodeById(rule.condition,input.dataset.editNode)
        || (rule.event?.id===input.dataset.editNode ? rule.event : null);
      if(node) node.value=input.value;
      updateTypedJson();
    };
  });

  $$("[data-remove-node]").forEach(btn=>{
    btn.onclick=()=>{
      const rule=findRule(btn.dataset.ruleId);
      if(!rule) return;
      const id=btn.dataset.removeNode;
      if(rule.event?.id===id) rule.event=null;
      else rule.condition=removeNodeById(rule.condition,id);
      renderRules();
    };
  });

  $$("[data-remove-action]").forEach(btn=>{
    btn.onclick=()=>{
      const rule=findRule(btn.dataset.ruleId);
      if(!rule) return;
      rule.actions.splice(Number(btn.dataset.removeAction),1);
      renderRules();
    };
  });
}

function showCompatibleSlots(out){
  $$(".slot").forEach(slot=>{
    const ok=slot.dataset.accept===out;
    slot.classList.toggle("compatible-hint",ok);
    slot.classList.toggle("incompatible-hint",!ok);
  });
  $$(".action-dropzone").forEach(zone=>{
    zone.classList.toggle("accepting",out==="action");
  });
}

function clearCompatibleSlots(){
  $$(".slot").forEach(slot=>slot.classList.remove("compatible-hint","incompatible-hint","accepting","rejected"));
  $$(".action-dropzone").forEach(zone=>zone.classList.remove("accepting"));
}

function bindTypedDnD(){
  $$("[data-node-id][draggable=true]").forEach(el=>{
    el.ondragstart=e=>{
      e.stopPropagation();
      const rule=findRule(el.dataset.ruleId);
      if(!rule) return;
      const id=el.dataset.nodeId;
      const node=rule.event?.id===id ? rule.event : getNodeById(rule.condition,id);
      if(!node) return;
      typedDrag={kind:"workspace-node",ruleId:rule.id,node:cloneNode(node),sourceId:id};
      showCompatibleSlots(node.out);
      e.dataTransfer.effectAllowed="move";
      e.dataTransfer.setData("text/plain","workspace-node");
    };
  });

  $$("[data-action-index][draggable=true]").forEach(el=>{
    el.ondragstart=e=>{
      const rule=findRule(el.dataset.ruleId);
      if(!rule) return;
      const idx=Number(el.dataset.actionIndex);
      typedDrag={kind:"workspace-action",ruleId:rule.id,node:cloneNode(rule.actions[idx]),sourceIndex:idx};
      showCompatibleSlots("action");
      e.dataTransfer.effectAllowed="move";
      e.dataTransfer.setData("text/plain","workspace-action");
    };
  });

  $$(".slot").forEach(slot=>{
    slot.ondragover=e=>{
      e.preventDefault();
      e.stopPropagation();
      if(!typedDrag) return;
      const ok=typedDrag.node?.out===slot.dataset.accept;
      slot.classList.toggle("accepting",ok);
      slot.classList.toggle("rejected",!ok);
      e.dataTransfer.dropEffect=ok?(typedDrag.kind==="palette"?"copy":"move"):"none";
    };

    slot.ondragleave=e=>{
      e.stopPropagation();
      slot.classList.remove("accepting","rejected");
    };

    slot.ondrop=e=>{
      e.preventDefault();
      e.stopPropagation();
      slot.classList.remove("accepting","rejected");

      const targetRule=findRule(slot.dataset.ruleId);
      if(!typedDrag || !targetRule) return;

      if(typedDrag.node?.out!==slot.dataset.accept){
        toast("这个积木类型不能放进这里");
        return;
      }

      // 工作区内移动：先从原规则摘除，再放入目标规则。
      if(typedDrag.kind==="workspace-node"){
        const sourceRule=findRule(typedDrag.ruleId);
        if(sourceRule){
          if(sourceRule.event?.id===typedDrag.sourceId) sourceRule.event=null;
          else sourceRule.condition=removeNodeById(sourceRule.condition,typedDrag.sourceId);
        }
      }

      setNodeAtPath(targetRule,slot.dataset.path,cloneNode(typedDrag.node));
      typedDrag=null;
      clearCompatibleSlots();
      renderRules();
    };
  });

  $$(".action-dropzone").forEach(zone=>{
    zone.ondragover=e=>{
      e.preventDefault();
      if(typedDrag?.node?.out==="action"){
        zone.classList.add("accepting");
        e.dataTransfer.dropEffect=typedDrag.kind==="palette"?"copy":"move";
      }
    };
    zone.ondragleave=()=>zone.classList.remove("accepting");
    zone.ondrop=e=>{
      e.preventDefault();
      zone.classList.remove("accepting");
      const targetRule=findRule(zone.dataset.ruleId);
      if(!typedDrag || !targetRule || typedDrag.node?.out!=="action"){
        toast("动作区只能放动作积木");
        return;
      }

      if(typedDrag.kind==="workspace-action"){
        const sourceRule=findRule(typedDrag.ruleId);
        if(sourceRule) sourceRule.actions.splice(typedDrag.sourceIndex,1);
      }

      targetRule.actions.push(cloneNode(typedDrag.node));
      typedDrag=null;
      clearCompatibleSlots();
      renderRules();
    };
  });

  $("#typedTrash").ondragover=e=>{
    if(!typedDrag || !String(typedDrag.kind).startsWith("workspace")) return;
    e.preventDefault();
    $("#typedTrash").classList.add("accepting");
  };
  $("#typedTrash").ondragleave=()=>$("#typedTrash").classList.remove("accepting");
  $("#typedTrash").ondrop=e=>{
    e.preventDefault();
    $("#typedTrash").classList.remove("accepting");
    if(!typedDrag) return;

    const sourceRule=findRule(typedDrag.ruleId);
    if(sourceRule){
      if(typedDrag.kind==="workspace-node"){
        if(sourceRule.event?.id===typedDrag.sourceId) sourceRule.event=null;
        else sourceRule.condition=removeNodeById(sourceRule.condition,typedDrag.sourceId);
      }else if(typedDrag.kind==="workspace-action"){
        sourceRule.actions.splice(typedDrag.sourceIndex,1);
      }
    }

    typedDrag=null;
    clearCompatibleSlots();
    renderRules();
    toast("积木已删除");
  };
}

// 积木库：每次拖拽复制一个新节点。
$$(".typed-palette [data-node]").forEach(el=>{
  el.ondragstart=e=>{
    const node=newNode(el.dataset.node);
    typedDrag={kind:"palette",node};
    showCompatibleSlots(node.out);
    e.dataTransfer.effectAllowed="copy";
    e.dataTransfer.setData("text/plain","palette");
  };
  el.ondragend=()=>{
    setTimeout(()=>{
      typedDrag=null;
      clearCompatibleSlots();
    },120);
  };
});

function visualNodeToAst(node){
  const simple={
    "text.reasoning":"reasoning","text.output":"final_answer","text.user":"user_message","text.system":"system_prompt",
    "number.reasoning_tokens":"reasoning_tokens","number.output_tokens":"output_tokens",
    "number.latency":"response_time_ms","number.http_status":"http_status"
  };
  if(simple[node.type]) return {type:simple[node.type]};
  if(node.type==="text.literal" || node.type==="pattern.literal") return {type:"literal",value:String(node.value??"")};
  if(node.type==="number.literal") return {type:"literal",value:Number(node.value)};
  const binary={
    "bool.contains":"contains","bool.not_contains":"not_contains","bool.equals_text":"text_equals",
    "bool.regex":"regex_matches","bool.gt":"greater_than","bool.lt":"less_than",
    "bool.gte":"greater_or_equal","bool.lte":"less_or_equal","bool.and":"and","bool.or":"or"
  };
  if(binary[node.type]){
    const result={type:binary[node.type]};
    for(const [key,value] of Object.entries(node.inputs||{})) result[key==="left"&&node.type==="bool.contains"?"text":key==="right"&&node.type==="bool.contains"?"value":key]=visualNodeToAst(value);
    if(node.type==="bool.not_contains") { result.text=result.left; result.value=result.right; delete result.left; delete result.right; }
    return result;
  }
  if(node.type==="bool.not") return {type:"not",value:visualNodeToAst(node.inputs.value)};
  throw new Error(`未知积木：${node.type}`);
}

function visualRuleToAst(rule){
  const eventMap={
    "event.standard_completed":["answer_completed","cheap"],"event.advanced_completed":["answer_completed","advanced"],
    "event.standard_finished":["request_finished","cheap"],"event.advanced_finished":["request_finished","advanced"]
  };
  const actionMap={"action.use_standard":"use_cheap_model","action.use_advanced":"use_advanced_model","action.retry":"retry_current_model","action.log":"log_only"};
  const [type,role]=eventMap[rule.event.type];
  return {id:rule.id,name:rule.name,event:{type,role},condition:visualNodeToAst(rule.condition),actions:rule.actions.map(action=>({type:actionMap[action.type]}))};
}

function astNodeToVisual(ast){
  const simple={reasoning:"text.reasoning",final_answer:"text.output",user_message:"text.user",system_prompt:"text.system",reasoning_tokens:"number.reasoning_tokens",output_tokens:"number.output_tokens",response_time_ms:"number.latency",http_status:"number.http_status"};
  if(simple[ast.type]) return newNode(simple[ast.type]);
  if(ast.type==="literal"){
    const type=typeof ast.value==="number"?"number.literal":"text.literal";
    return newNode(type,{value:String(ast.value)});
  }
  const binary={contains:"bool.contains",not_contains:"bool.not_contains",text_equals:"bool.equals_text",regex_matches:"bool.regex",greater_than:"bool.gt",less_than:"bool.lt",greater_or_equal:"bool.gte",less_or_equal:"bool.lte",and:"bool.and",or:"bool.or"};
  if(binary[ast.type]){
    const node=newNode(binary[ast.type]);
    for(const [key,value] of Object.entries(ast)) if(key!=="type") node.inputs[key==="text"&&["contains","not_contains"].includes(ast.type)?"left":key==="value"&&["contains","not_contains"].includes(ast.type)?"right":key]=astNodeToVisual(value);
    if(ast.type==="regex_matches" && ast.pattern?.type==="literal") node.inputs.pattern=newNode("pattern.literal",{value:ast.pattern.value});
    return node;
  }
  if(ast.type==="not"){ const node=newNode("bool.not"); node.inputs.value=astNodeToVisual(ast.value); return node; }
  throw new Error(`未知 AST 节点：${ast.type}`);
}

function astRuleToVisual(rule){
  const event=`event.${rule.event.role==="cheap"?"standard":"advanced"}_${rule.event.type==="answer_completed"?"completed":"finished"}`;
  const actionMap={use_cheap_model:"action.use_standard",use_advanced_model:"action.use_advanced",retry_current_model:"action.retry",log_only:"action.log"};
  return {id:rule.id,name:rule.name,event:newNode(event),condition:astNodeToVisual(rule.condition),actions:rule.actions.map(action=>newNode(actionMap[action.type]))};
}

function updateTypedJson(){
  $("#typedJsonPreview").textContent=JSON.stringify({
    format:"modelgate-visual-rules",
    version:1,
    rules:typedRules.map(visualRuleToAst)
  },null,2);
}

$("#addRuleProgram").onclick=()=>{
  typedRules.push(newRule(`规则 ${typedRules.length+1}`));
  renderRules();
  toast("已新建一条独立规则");
};

$("#typedLoadExample").onclick=()=>{
  typedRules=[];

  const r1=newRule("低价模型输出异常检查");
  r1.event=newNode("event.standard_completed");

  const c1=newNode("bool.regex");
  c1.inputs.text=newNode("text.output");
  c1.inputs.pattern=newNode("pattern.literal",{value:"(?i)(无法完成|无法确定|信息不足)"});
  r1.condition=c1;
  r1.actions=[newNode("action.use_advanced"),newNode("action.log")];

  const r2=newRule("高级模型输出完整性检查");
  r2.event=newNode("event.advanced_completed");

  const shortOutput=newNode("bool.lt");
  shortOutput.inputs.left=newNode("number.output_tokens");
  shortOutput.inputs.right=newNode("number.literal",{value:"80"});

  const missingEnding=newNode("bool.not_contains");
  missingEnding.inputs.left=newNode("text.output");
  missingEnding.inputs.right=newNode("text.literal",{value:"。"});

  const c2=newNode("bool.and");
  c2.inputs.left=shortOutput;
  c2.inputs.right=missingEnding;
  r2.condition=c2;
  r2.actions=[newNode("action.log")];

  typedRules.push(r1,r2);
  renderRules();
  toast("已载入双规则参考示例");
};

$("#typedTest").onclick=async()=>{
  const issues=[];

  function validateNode(node,ruleName){
    if(!node) return;
    const def=NODE_DEFS[node.type];
    if(def?.inputs){
      for(const [name] of def.inputs){
        if(!node.inputs?.[name]) issues.push(`${ruleName}：${def.label} 缺少输入`);
        else validateNode(node.inputs[name],ruleName);
      }
    }
    if(def?.editable && String(node.value??"").trim()===""){
      issues.push(`${ruleName}：${def.label} 的输入为空`);
    }
  }

  typedRules.forEach(rule=>{
    if(!rule.event) issues.push(`${rule.name}：缺少触发事件`);
    if(!rule.condition) issues.push(`${rule.name}：缺少条件`);
    else validateNode(rule.condition,rule.name);
    if(!rule.actions.length) issues.push(`${rule.name}：缺少动作`);
  });

  if(!typedRules.length){
    issues.push("还没有任何规则");
  }

  if(issues.length){
    $("#typedTestResult").textContent="还有问题："+issues.slice(0,5).join("；");
    $("#typedTestResult").className="test-result";
  }else{
    try{
      const response=await fetch("/api/rules",{method:"PUT",headers:{"Content-Type":"application/json"},body:JSON.stringify(typedRules.map(visualRuleToAst))});
      if(!response.ok){ const data=await response.json(); throw new Error(data?.error?.message||"后端校验失败"); }
      $("#typedTestResult").textContent=`✓ ${typedRules.length} 条规则已通过后端校验并保存`;
      $("#typedTestResult").className="test-result success";
    }catch(error){ $("#typedTestResult").textContent=error.message; $("#typedTestResult").className="test-result"; }
  }
};

$("#typedToggleJson").onclick=()=>{
  $("#typedJsonPreview").classList.toggle("show");
  $("#typedToggleJson").textContent=$("#typedJsonPreview").classList.contains("show")
    ?"隐藏底层结构":"查看底层结构";
};

renderRules();
fetch("/api/rules")
  .then(response=>response.ok?response.json():Promise.reject(new Error("读取规则失败")))
  .then(rules=>{ typedRules=rules.map(astRuleToVisual); renderRules(); })
  .catch(error=>{ $("#typedTestResult").textContent=error.message; });

// 手动规则导入 / 导出

function downloadTextFile(filename, content, mime="application/json"){
  const blob=new Blob([content],{type:mime});
  const url=URL.createObjectURL(blob);
  const a=document.createElement("a");
  a.href=url;
  a.download=filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(()=>URL.revokeObjectURL(url),500);
}

$("#exportManualRules").addEventListener("click",()=>{
  const payload={
    format:"modelgate-visual-rules",
    version:1,
    exported_at:new Date().toISOString(),
    rules:typedRules.map(visualRuleToAst)
  };

  downloadTextFile(
    "modelgate-rules.mgrule",
    JSON.stringify(payload,null,2),
    "application/json"
  );

  $("#manualImportStatus").textContent=`已导出 ${typedRules.length} 条规则。`;
  toast("规则已导出");
});

$("#importManualRules").addEventListener("click",()=>{
  $("#manualRulesFile").click();
});

$("#manualRulesFile").addEventListener("change",async e=>{
  const file=e.target.files?.[0];
  if(!file) return;

  try{
    const raw=await file.text();
    const data=JSON.parse(raw);

    const response=await fetch("/api/rules/import",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(data)});
    if(!response.ok){ const detail=await response.json().catch(()=>null); throw new Error(detail?.error?.message||"后端拒绝导入"); }
    const imported=data.rules.length;
    const allRules=await fetch("/api/rules").then(response=>response.json());
    typedRules=allRules.map(astRuleToVisual);

    renderRules();
    $("#manualImportStatus").textContent=`已从 ${file.name} 导入 ${imported} 条规则（追加到当前列表）。`;
    toast("规则导入成功");
  }catch(err){
    $("#manualImportStatus").textContent="导入失败："+err.message;
    toast("规则导入失败");
  }finally{
    e.target.value="";
  }
});

// AI Markdown 导出

$("#exportMarkdownBtn").addEventListener("click",()=>{
  const content=$("#judgeMarkdown").value || "";
  downloadTextFile("modelgate-ai-supervisor.md",content,"text/markdown;charset=utf-8");
  $("#markdownFileName").textContent="已导出 modelgate-ai-supervisor.md";
  toast("Markdown 已导出");
});

// 三级规则删除确认

let pendingDeleteRuleId=null;

function showDeleteStage(stage){
  [1,2,3].forEach(n=>{
    $("#deleteStage"+n).classList.toggle("active",n===stage);
  });
}

function resetDeleteSwipe(){
  const knob=$("#deleteSwipeKnob");
  knob.style.left="50%";
  knob.style.transform="translate(-50%,-50%)";
}

function closeDeleteRule(){
  pendingDeleteRuleId=null;
  resetDeleteSwipe();
  $("#deleteRuleOverlay").classList.remove("show");
  showDeleteStage(1);
}

function beginDeleteRule(ruleId){
  pendingDeleteRuleId=ruleId;
  showDeleteStage(1);
  resetDeleteSwipe();
  $("#deleteRuleOverlay").classList.add("show");
}

$("#deleteStage1Cancel").onclick=closeDeleteRule;
$("#deleteStage2Cancel").onclick=closeDeleteRule;

$("#deleteStage1Confirm").onclick=()=>{
  showDeleteStage(2);
};

$("#deleteStage2Confirm").onclick=()=>{
  showDeleteStage(3);
  requestAnimationFrame(resetDeleteSwipe);
};

// 第三级确认：滑块从正中间开始。
// 向左达到约 8%：取消。
// 向右达到约 92%：真正删除。
// 松手但没有达到阈值，则自动回到中间。
(() => {
  const track=$("#deleteSwipeTrack");
  const knob=$("#deleteSwipeKnob");
  let dragging=false;

  function setKnobByClientX(clientX){
    const rect=track.getBoundingClientRect();
    const knobHalf=24;
    const min=knobHalf;
    const max=rect.width-knobHalf;
    let x=clientX-rect.left;
    x=Math.max(min,Math.min(max,x));

    const percent=(x/rect.width)*100;
    knob.style.left=percent+"%";
    knob.style.transform="translate(-50%,-50%)";

    return percent;
  }

  function finish(percent){
    dragging=false;
    knob.classList.remove("dragging");

    if(percent>=88){
      const id=pendingDeleteRuleId;
      typedRules=typedRules.filter(r=>r.id!==id);
      closeDeleteRule();
      renderRules();
      toast("规则已废弃");
      return;
    }

    if(percent<=12){
      closeDeleteRule();
      toast("已取消删除");
      return;
    }

    knob.animate(
      [{left:knob.style.left},{left:"50%"}],
      {duration:180,easing:"ease-out"}
    );
    knob.style.left="50%";
  }

  knob.addEventListener("pointerdown",e=>{
    dragging=true;
    knob.classList.add("dragging");
    knob.setPointerCapture(e.pointerId);
    e.preventDefault();
  });

  knob.addEventListener("pointermove",e=>{
    if(!dragging) return;
    setKnobByClientX(e.clientX);
  });

  knob.addEventListener("pointerup",e=>{
    if(!dragging) return;
    const percent=setKnobByClientX(e.clientX);
    finish(percent);
  });

  knob.addEventListener("pointercancel",()=>{
    if(!dragging) return;
    finish(50);
  });
})();



// Markdown 在浏览器中读取后随配置保存，不会上传到任何外部服务。
$("#uploadMarkdownBtn").addEventListener("click",()=>{
  $("#markdownFileInput").click();
});

$("#markdownFileInput").addEventListener("change", async (e)=>{
  const file=e.target.files && e.target.files[0];
  if(!file) return;

  $("#markdownFileName").textContent=file.name;

  try{
    const content=await file.text();
    $("#judgeMarkdown").value=content;
    toast("Markdown 已载入");
  }catch(err){
    toast("读取 Markdown 失败");
  }
});

$("#fillMarkdownExample").addEventListener("click",()=>{
  $("#judgeMarkdown").value=`# ModelGate AI 监管规则

## Prompt

你是 ModelGate 的监管模型。

你的任务不是回答原始用户问题，而是判断低价模型的思考或回答是否出现明显异常。

重点检查：

- 是否严重偏离用户当前请求
- 是否出现明显上下文漂移
- 是否把示例内容误认为真实事实
- 是否围绕无关问题反复推理
- 是否产生与上下文明显矛盾的信息
- 是否出现明显错误关联
- 是否违反用户明确要求

正常时只输出：

SAFE

需要专业模型接管时只输出：

UNSAFE

不要解释。

## Match

\`\`\`regex
(?i)^UNSAFE$
\`\`\`
`;
  $("#markdownFileName").textContent="使用内置示例";
  toast("已填入 Markdown 示例");
});

async function saveLocalSupervisor(){
  if(!appConfig) await loadAppConfig();
  appConfig.local_ai_supervisor={markdown:$("#judgeMarkdown").value};
  await saveAppConfig();
}

$("#saveLocalSupervisor").addEventListener("click",async()=>{
  try{ await saveLocalSupervisor(); closeOverlay("localOverlay"); toast("本地监管规则已保存"); }
  catch(error){ toast(error.message); }
});

$("#fakeJudgeTest").addEventListener("click",async()=>{
  $("#judgeTestResult").textContent="正在请求监管模型进行结构化判断……";
  $("#judgeTestResult").className="test-result";
  try{
    await saveLocalSupervisor();
    const response=await fetch("/api/supervisor/test",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify({answer:"这是一段用于验证监管链路的候选回答。"})});
    if(!response.ok){ const data=await response.json().catch(()=>null); throw new Error(data?.error?.message||"连接失败"); }
    const result=await response.json();
    $("#judgeTestResult").textContent=`✓ 监管链路有效；测试判断：${result.escalate?"需要升级":"无需升级"}`;
    $("#judgeTestResult").className="test-result success";
  }catch(error){ $("#judgeTestResult").textContent=error.message; }
});

// 本地服务设置
function serverPreview(){
  const host=$("#listenHost").value || "127.0.0.1";
  const port=$("#listenPort").value || "8181";
  const model=$("#virtualModel").value || "modelgate-auto";
  $("#serverPreview").textContent=
    `Base URL：http://${host}:${port}/v1　｜　Model：${model}`;
}
$("#serverPreviewBtn").addEventListener("click",serverPreview);
$("#saveServerBtn").addEventListener("click",async()=>{
  serverPreview();
  if(!appConfig) await loadAppConfig();
  appConfig.server.port=Number($("#listenPort").value||8181);
  appConfig.server.virtual_model=$("#virtualModel").value||"modelgate-auto";
  try{ await saveAppConfig(); }catch(error){ toast(error.message); return; }
  closeOverlay("serverOverlay");
  toast("本地服务设置已保存；端口修改将在下次启动生效");
  if(running){
    // 运行中时同步主界面展示
    const host=$("#listenHost").value || "127.0.0.1";
    const port=$("#listenPort").value || "8181";
    const vmodel=$("#virtualModel").value || "modelgate-auto";
    const base=`http://${host}:${port}/v1`;
    $("#localBase").textContent=base;
    $("#localModel").textContent=vmodel;
    $("#localChat").textContent=base+"/chat/completions";
    $("#localModels").textContent=base+"/models";
  }
});

$("#aboutPing").addEventListener("click",()=>{
  $("#aboutPingResult").textContent="✓ 这个按钮也能点，哼。";
  $("#aboutPingResult").className="test-result success";
});

updateModeUI();
serverPreview();
