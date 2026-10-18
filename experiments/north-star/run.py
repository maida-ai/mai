#!/usr/bin/env python3
"""Measure delivery mechanics against a disposable mai authority."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import time


PROMPT = """Implement one independent Python statistics module named agent_{n}.py in this workspace. It must define solve(values) returning a dict with count, total, min, max. Empty input contract: count=0, total=0, min=null, max=null. Use integer inputs. Commit your work and submit it to mai with the configured remote using agent identity agent_{n}. Do not run mai integrate. Do not modify mai.toml or gate.sh; those are trusted harness files. Push the commit to remote mai at refs/for/main and provide the required Git push option -o agent=agent_{n}. Do not coordinate with other agents; this task is independent."""
GATE = r'''set -eu
cd "$MAI_CANDIDATE_ROOT"
python3 - "$MAI_CANDIDATE_ROOT" <<'PY'
import importlib.util, pathlib, sys
root=pathlib.Path(sys.argv[1]); files=sorted(root.glob("agent_*.py"))
for path in files:
    spec=importlib.util.spec_from_file_location(path.stem,path)
    mod=importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
    assert callable(getattr(mod,"solve",None)), path
    assert mod.solve([2,-1,4]) == {"count":3,"total":5,"min":-1,"max":4}, path
    assert mod.solve([]) == {"count":0,"total":0,"min":None,"max":None}, path
PY
'''
POLICY = '''version = 1\n[check]\nprogram = "sh"\nscript = "gate.sh"\ntimeout_ms = 30000\noutput_tail_bytes = 4096\n[validators]\nslots = 1\nport_base = 32100\nports_per_slot = 10\n'''


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("out", type=Path); p.add_argument("--mai", type=Path, required=True)
    p.add_argument("--agents", type=int, required=True); p.add_argument("--runner", type=Path, required=True)
    p.add_argument("--timeout", type=int, default=900)
    p.add_argument("--integration", choices=("explicit", "observe"), default="explicit")
    a=p.parse_args()
    if not 1 <= a.agents <= 30: p.error("--agents must be between 1 and 30")
    if a.timeout < 1: p.error("--timeout must be positive")
    out=a.out.resolve(); out.mkdir(parents=True, exist_ok=False)
    binary=a.mai.resolve(strict=True); runner=a.runner.resolve(strict=True)
    env=dict(os.environ, GIT_TERMINAL_PROMPT="0", GIT_AUTHOR_NAME="north-star", GIT_COMMITTER_NAME="north-star", GIT_AUTHOR_EMAIL="north-star@example.invalid", GIT_COMMITTER_EMAIL="north-star@example.invalid")
    logs=out/"logs"; logs.mkdir()
    records={"setup":[],"observation":[],"operator":[],"agents":[]}
    failures=[]; unfinished=[]; landed=[]; interventions=[]; retained=[]; broken=[]
    source_head_before=source_head_after=None; project=None; engine_owned=False
    def call(group,label,cmd,cwd=None,timeout=120,check=True):
        start=time.monotonic(); path=logs/(label.replace("/","_")+".log")
        code=None
        with path.open("wb") as f:
            proc=subprocess.Popen([str(x) for x in cmd],cwd=cwd,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
            try: code=proc.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                try: os.killpg(proc.pid,signal.SIGTERM); proc.wait(timeout=3)
                except subprocess.TimeoutExpired: os.killpg(proc.pid,signal.SIGKILL); proc.wait()
                code="timeout"
        row={"label":label,"command":[str(x) for x in cmd],"cwd":str(cwd) if cwd else None,"elapsed_s":time.monotonic()-start,"exit":code,"log":str(path.relative_to(out))}
        records[group].append(row)
        if check and code != 0: raise RuntimeError(f"{label} failed ({code}); see {path}")
        return path.read_text(errors="replace")
    def git(repo,*args): return call("setup","git-"+str(len(records["setup"])),["git","-C",repo,*args]).strip()
    def mai(*args,group="setup",label=None,**kw):
        return call(group,label or "mai-"+str(len(records[group])),[binary,"--json",*args],**kw).strip()
    def queue_all(label):
        items=[]; cursor=None
        while True:
            cmd=[binary,"--json","queue",project,"--limit","100"]
            if cursor: cmd += ["--after",f"{cursor['change_id']}:{cursor['generation']}"]
            page=json.loads(call("observation",label+"-page-"+str(len(items)//100),cmd))
            items.extend(page.get("items",[])); cursor=page.get("next_cursor")
            if not cursor: return items

    summary={"schema_version":2,"baseline":1.0,"integration_mode":a.integration,"landed":landed,"attempted":0,"received":0,"failed":failures,"unfinished":unfinished,"operator_interventions_per_landed_change":None,"operator_surrogate_command_count":0,"operator_surrogate_commands":interventions,"guardrails":{},"measurement_note":"Synthetic task repository; fake-runner mechanics are not representative real-agent results. Explicit mode counts harness operator-surrogate integrate calls. Observe mode only waits for externally configured automatic integration; mai does not implement product automation here."}
    try:
        source=out/"source"; source.mkdir(); call("setup","git-init",["git","-C",source,"init","-b","main"])
        git(source,"config","user.name","north-star"); git(source,"config","user.email","north-star@example.invalid")
        (source/"gate.sh").write_text(GATE); (source/"gate.sh").chmod(0o755); (source/"mai.toml").write_text(POLICY)
        (source/"README.txt").write_text("North-star disposable synthetic agent task.\n")
        git(source,"add","."); git(source,"commit","-m","Seed trusted independent-agent gate")
        source_head_before=git(source,"rev-parse","HEAD")
        project=out/"authority"; mai("init",project,"--from",source); engine_owned=True
        call("setup","receive-enable",[binary,"--json","receive","enable",project,"--max-input-bytes","10485760","--max-file-bytes","1048576","--max-changed-files","100"])
        trunk0=call("observation","baseline-trunk",["git","--git-dir",project/"authority.git","rev-parse","refs/heads/trunk"]).strip()
        baseline_gate=json.loads(mai("validator","run",project,trunk0,group="observation"))
        if baseline_gate.get("outcome")!="success": failures.append({"guardrail":"baseline_gate_failed","outcome":baseline_gate.get("outcome")})
        workspaces=[]
        for i in range(1,a.agents+1):
            w=out/f"workspace-{i}"; workspaces.append(w); mai("workspace",project,f"agent_{i}",w)
            (out/f"prompt-{i}.txt").write_text(PROMPT.format(n=i))
        def agent(i,w):
            call("agents",f"agent-{i}",[runner,w,out/f"prompt-{i}.txt"],timeout=a.timeout,check=False)
        started=time.monotonic()
        with concurrent.futures.ThreadPoolExecutor(max_workers=a.agents) as pool:
            list(pool.map(lambda iw: agent(*iw),enumerate(workspaces,1)))
        agent_elapsed=time.monotonic()-started
        workspace_heads={str(w): git(w,"rev-parse","HEAD") for w in workspaces}
        entries=queue_all("queue"); (out/"received.json").write_text(json.dumps(entries,indent=2)+"\n")
        received=[e for e in entries if isinstance(e,dict) and e.get("state")=="received"]
        summary["received"]=len(received); summary["attempted"]=len(records["agents"])
        known={f"agent_{i}" for i in range(1,a.agents+1)}
        submitted={e.get("agent") for e in entries if isinstance(e,dict)}
        missing_agents=sorted(known-submitted)
        if missing_agents: failures.append({"guardrail":"agents_without_submission","agents":missing_agents})
        for e in entries:
            oid=e.get("commit") if isinstance(e,dict) else None
            if oid:
                retained.append({"identity":e.get("id"),"submitted_oid":oid,"exists":subprocess.run(["git","--git-dir",str(project/"authority.git"),"cat-file","-e",oid]).returncode==0})
        if a.integration=="explicit":
            for e in received:
                ident=e.get("id",{}); change=ident.get("change_id"); generation=ident.get("generation",1)
                before=time.monotonic(); row={"identity":ident,"commands":[]}
                try:
                    row["commands"].append("integrate")
                    engine_owned=True
                    mai("integrate",project,change,"--generation",str(generation),"--mode","textual","--wait-seconds",str(a.timeout),group="operator",label=f"integrate-{change}-{generation}",timeout=a.timeout+30)
                    detail=json.loads(mai("show",project,change,"--generation",str(generation),group="observation",label=f"show-{change}-{generation}"))
                    state=detail.get("state",{}); outcome=state.get("outcome",{}).get("kind") if state.get("kind")=="terminal" else state.get("kind")
                    if outcome=="landed": landed.append({"identity":ident,"submitted_commit":detail.get("commit"),"landed_commit":state.get("outcome",{}).get("commit")})
                    else: failures.append({"identity":ident,"outcome":outcome})
                except Exception as ex: failures.append({"identity":ident,"error":str(ex)})
                row["elapsed_s"]=time.monotonic()-before; interventions.append(row)
        else:
            deadline=time.monotonic()+min(a.timeout,300)
            while time.monotonic()<deadline:
                current=queue_all("observe-queue")
                if all(e.get("state")=="terminal" for e in current): break
                time.sleep(1)
        finals=queue_all("final-queue")
        for e in finals:
            state=e.get("state"); outcome=e.get("outcome") or {}; kind=outcome.get("kind") if isinstance(outcome,dict) else None
            if state=="terminal" and kind=="landed":
                if not any(x["identity"]==e["id"] for x in landed):
                    detail=json.loads(mai("show",project,e["id"]["change_id"],"--generation",str(e["id"]["generation"]),group="observation"))
                    landed.append({"identity":e["id"],"submitted_commit":detail["commit"],"landed_commit":detail["state"]["outcome"]["commit"]})
                continue
            row={"identity":e.get("id"),"state":state,"outcome":kind}
            (failures if state in ("rejected","cancelled") or kind in ("rejected","cancelled") else unfinished).append(row)
        trunk1=call("observation","final-trunk",["git","--git-dir",project/"authority.git","rev-parse","refs/heads/trunk"]).strip()
        candidate=json.loads(mai("validator","run",project,trunk1,group="observation",label="final-gate",check=False))
        if candidate.get("outcome")!="success": broken.append(trunk1)
        for index, row in enumerate(landed):
            oid=row.get("landed_commit")
            if oid:
                result=json.loads(mai("validator","run",project,oid,group="observation",label=f"landed-gate-{index}",check=False))
                row["gate"]=result.get("outcome")
                if row["gate"]!="success": broken.append(oid)
        for item in retained:
            item["exists"]=subprocess.run(["git","--git-dir",str(project/"authority.git"),"cat-file","-e",item["submitted_oid"]]).returncode==0
        for w, oid in workspace_heads.items():
            if git(w,"rev-parse","HEAD") != oid: failures.append({"guardrail":"agent_head_changed","workspace":w})
        summary["task_start_to_observation_s"]=time.monotonic()-started
        summary["agent_workspace_heads"]=workspace_heads
        source_head_after=git(source,"rev-parse","HEAD")
        if source_head_after != source_head_before: failures.append({"guardrail":"source_workspace_head_changed","before":source_head_before,"after":source_head_after})
        accepted=out/"accepted"; mai("workspace",project,"north_star_audit",accepted,group="observation",label="accepted-workspace")
        actual={p.name for p in accepted.glob("agent_*.py")}; expected={f"agent_{i}.py" for i in range(1,a.agents+1)}
        identity_ok=actual==expected
        module_check={"expected":sorted(expected),"actual":sorted(actual),"complete":identity_ok}
        if not identity_ok: failures.append({"guardrail":"expected_module_identity","expected":sorted(expected),"actual":sorted(actual)})
        if broken: failures.append({"guardrail":"landed_gate_failed","commits":broken})
        if any(not x["exists"] for x in retained): failures.append({"guardrail":"submitted_oid_not_retained"})
        failed_runners=[r["label"] for r in records["agents"] if r["exit"] != 0]
        failures.extend({"runner":name,"reason":"runner exited unsuccessfully"} for name in failed_runners)
        summary.update({"landed":landed,"attempted":len(records["agents"]),"received":len(received),"agent_elapsed_s":agent_elapsed,"operator_surrogate_command_count":sum(len(x["commands"]) for x in interventions),"operator_interventions_per_landed_change":sum(len(x["commands"]) for x in interventions)/len(landed) if landed else None,"guardrails":{"baseline_gate":baseline_gate.get("outcome"),"fresh_final_gate":candidate.get("outcome"),"retained_source_oids":retained,"broken_trunk_landings":broken,"expected_modules":module_check},"queue_time":"unavailable; receipt time is not exposed by the current CLI"})
    except Exception as ex:
        failures.append({"harness_error":str(ex)})
    finally:
        if project and engine_owned:
            call("setup","stop-owned-engine",[binary,"--json","stop",project],check=False)
        (out/"commands.json").write_text(json.dumps(records,indent=2)+"\n")
        (out/"summary.json").write_text(json.dumps(summary,indent=2)+"\n")
        (out/"provenance.json").write_text(json.dumps({"mai":str(binary),"mai_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),"runner":str(runner),"runner_sha256":hashlib.sha256(runner.read_bytes()).hexdigest(),"expected_agents":a.agents,"integration":a.integration},indent=2)+"\n")
    print(json.dumps(summary,indent=2))
    if failures or unfinished: raise SystemExit(1)

if __name__=="__main__": main()
