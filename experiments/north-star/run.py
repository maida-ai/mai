#!/usr/bin/env python3
"""Run independent coding agents against a disposable mai authority."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time


PROMPT = """Implement one independent Python statistics module named agent_{n}.py in this workspace. It must define solve(values) returning a dict with count, total, min, max. Empty input contract: count=0, total=0, min=null, max=null. Use integer inputs. Add or update tests if useful. Commit your work and submit it to mai with the configured remote using agent identity agent_{n}. Do not run mai integrate. Decide your own implementation, tests, and ordinary Git commands. Do not coordinate with other agents; this task is independent."""
GATE = r'''set -eu
cd "$MAI_CANDIDATE_ROOT"
python3 - "$MAI_CANDIDATE_ROOT" <<'PY'
import importlib.util, pathlib, sys
root=pathlib.Path(sys.argv[1])
files=sorted(root.glob("agent_*.py"))
for path in files:
    spec=importlib.util.spec_from_file_location(path.stem,path)
    mod=importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
    solve=mod.solve
    assert solve([2,-1,4]) == {"count":3,"total":5,"min":-1,"max":4}, path
    assert solve([]) == {"count":0,"total":0,"min":None,"max":None}, path
PY
'''
POLICY = '''version = 1\n[check]\nprogram = "sh"\nscript = "gate.sh"\ntimeout_ms = 30000\noutput_tail_bytes = 4096\n[validators]\nslots = 1\nport_base = 32100\nports_per_slot = 10\n'''


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("out", type=Path); p.add_argument("--mai", type=Path, required=True)
    p.add_argument("--agents", type=int, required=True); p.add_argument("--runner", type=Path, required=True)
    p.add_argument("--timeout", type=int, default=900)
    a=p.parse_args()
    if a.agents < 1 or a.timeout < 1: p.error("--agents and --timeout must be positive")
    out=a.out.resolve(); out.mkdir(parents=True, exist_ok=False)
    binary=a.mai.resolve(strict=True); runner=a.runner.resolve(strict=True)
    env=dict(os.environ, GIT_TERMINAL_PROMPT="0", GIT_AUTHOR_NAME="north-star", GIT_COMMITTER_NAME="north-star", GIT_AUTHOR_EMAIL="north-star@example.invalid", GIT_COMMITTER_EMAIL="north-star@example.invalid")
    logs=out/"logs"; logs.mkdir()
    records={"setup":[],"observation":[],"operator":[],"agents":[]}
    def call(group,label,cmd,cwd=None,timeout=120,check=True):
        start=time.monotonic(); path=logs/(label.replace("/","_")+".log")
        try:
            with path.open("wb") as f: r=subprocess.run([str(x) for x in cmd],cwd=cwd,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=timeout)
            code=r.returncode
        except subprocess.TimeoutExpired: code="timeout"
        row={"label":label,"command":[str(x) for x in cmd],"cwd":str(cwd) if cwd else None,"elapsed_s":time.monotonic()-start,"exit":code,"log":str(path.relative_to(out))}
        records[group].append(row)
        if check and code != 0: raise RuntimeError(f"{label} failed ({code}); see {path}")
        return path.read_text(errors="replace")
    def git(repo,*args): return call("setup","setup-git-"+str(len(records["setup"])),["git","-C",repo,*args]).strip()
    def mai(*args): return call("setup","setup-mai-"+str(len(records["setup"])),[binary,"--json",*args]).strip()

    source=out/"source"; source.mkdir(); call("setup","git-init",["git","-C",source,"init","-b","main"])
    git(source,"config","user.name","north-star"); git(source,"config","user.email","north-star@example.invalid")
    (source/"gate.sh").write_text(GATE); (source/"gate.sh").chmod(0o755); (source/"mai.toml").write_text(POLICY)
    (source/"README.txt").write_text("North-star disposable agent task.\n")
    git(source,"add","."); git(source,"commit","-m","Seed trusted independent-agent gate")
    baseline=git(source,"rev-parse","HEAD")
    project=out/"authority"; mai("init",project,"--from",source)
    call("setup","receive-enable",[binary,"--json","receive","enable",project,"--max-input-bytes","10485760","--max-file-bytes","1048576","--max-changed-files","100"])
    trunk0=call("observation","baseline-trunk",["git","--git-dir",project/"authority.git","rev-parse","refs/heads/trunk"]).strip()
    baseline_gate=json.loads(mai("validator","run",project,trunk0))
    workspaces=[]
    for i in range(1,a.agents+1):
        w=out/f"workspace-{i}"; workspaces.append(w)
        mai("workspace",project,f"agent_{i}",w)
        (out/f"prompt-{i}.txt").write_text(PROMPT.format(n=i))
    def agent(i,w):
        return call("agents",f"agent-{i}",[runner,w,out/f"prompt-{i}.txt"],timeout=a.timeout,check=False)
    started=time.monotonic()
    with concurrent.futures.ThreadPoolExecutor(max_workers=a.agents) as pool:
        futures=[pool.submit(agent,i,w) for i,w in enumerate(workspaces,1)]
        for f in futures: f.result()
    agent_elapsed=time.monotonic()-started
    queue=json.loads(call("observation","queue",[binary,"--json","queue",project,"--limit",str(max(50,a.agents*4))]))
    entries=queue.get("items",[])
    # Persist exact identities before acting on any generation.
    (out/"received.json").write_text(json.dumps(queue,indent=2)+"\n")
    received=[]
    for e in entries:
        if isinstance(e,dict) and e.get("state",{}).get("kind")=="received": received.append(e)
    interventions=[]; landed=[]; failures=[]; unfinished=[]
    for e in received:
        gen=e.get("id",{}); change=gen.get("change_id"); generation=gen.get("generation",1)
        ident=f"{change}:{generation}"
        before=time.monotonic()
        try:
            call("operator",f"inspect-before-{ident}",[binary,"--json","show",project,str(change),"--generation",str(generation)])
            call("operator",f"integrate-{ident}",[binary,"--json","integrate",project,str(change),"--generation",str(generation),"--mode","textual","--wait-seconds",str(a.timeout)],timeout=a.timeout+30)
            detail=json.loads(call("observation",f"inspect-after-{ident}",[binary,"--json","show",project,str(change),"--generation",str(generation)]))
            state=detail.get("state",{})
            outcome=state.get("outcome",{}).get("kind") if state.get("kind")=="terminal" else state.get("kind")
            if outcome=="landed":
                oid=detail.get("commit")
                landed.append({"identity":ident,"elapsed_s":time.monotonic()-before,"submitted_commit":oid,"landed_commit":state.get("outcome",{}).get("commit")})
                if oid: call("observation",f"retain-{ident}",["git","--git-dir",project/"authority.git","cat-file","-e",oid])
            else: failures.append({"identity":ident,"outcome":detail.get("outcome")})
        except Exception as ex: failures.append({"identity":ident,"error":str(ex)})
        interventions.append({"identity":ident,"required_commands":["show","integrate"],"elapsed_s":time.monotonic()-before})
    # Re-read all generations: unintegrated, rejected, or still processing work counts distinctly.
    final=json.loads(call("observation","final-queue",[binary,"--json","queue",project,"--limit",str(max(50,a.agents*4))]))
    finals=final.get("items",[])
    for e in finals:
        if isinstance(e,dict) and e.get("state",{}).get("kind")!="landed":
            state=e.get("state",{}); kind=state.get("outcome",{}).get("kind") if state.get("kind")=="terminal" else state.get("kind")
            (failures if kind in ("rejected","cancelled") else unfinished).append({"identity":e.get("id"),"outcome":kind})
    broken=[]
    trunk1=call("observation","final-trunk",["git","--git-dir",project/"authority.git","rev-parse","refs/heads/trunk"]).strip()
    candidate=json.loads(call("observation","final-gate",[binary,"--json","validator","run",project,trunk1]))
    if candidate.get("outcome")!="success": broken=[trunk1]
    retained=[]
    for w in workspaces:
        retained.append({"workspace":w.name,"head":git(w,"rev-parse","HEAD"),"status":git(w,"status","--porcelain")})
    expected={f"agent_{i}.py" for i in range(1,a.agents+1)}
    accepted=out/"accepted"; call("observation","accepted-workspace",[binary,"--json","workspace",project,"north_star_audit",accepted])
    modules={p.name for p in accepted.glob("agent_*.py")}
    module_check={"expected":sorted(expected),"actual":sorted(modules),"complete":expected==modules}
    failed_runners=[r["label"] for r in records["agents"] if r["exit"] != 0]
    if failed_runners: failures.extend({"runner":name,"reason":"runner exited unsuccessfully"} for name in failed_runners)
    command_count=sum(len(x["required_commands"]) for x in interventions)
    summary={"schema_version":1,"baseline":1.0,"operator_interventions_per_landed_change":command_count/len(landed) if landed else None,"landed":landed,"attempted":len(records["agents"]),"received":len(received),"failed":failures,"unfinished":unfinished,"agent_elapsed_s":agent_elapsed,"guardrails":{"baseline_gate":baseline_gate.get("outcome"),"fresh_final_gate":candidate.get("outcome"),"retained_source_oids":retained,"broken_trunk_landings":broken,"expected_modules":module_check},"operator_surrogate_command_count":command_count,"operator_surrogate_commands":interventions,"measurement_note":"Counts required show/integrate commands as operator interventions; this is operational command count, not observed human effort. Agent-runner exit status alone is not landing success."}
    (out/"commands.json").write_text(json.dumps(records,indent=2)+"\n")
    (out/"summary.json").write_text(json.dumps(summary,indent=2)+"\n")
    (out/"provenance.json").write_text(json.dumps({"mai":str(binary),"mai_sha256":hashlib.sha256(binary.read_bytes()).hexdigest(),"runner":str(runner),"runner_sha256":hashlib.sha256(runner.read_bytes()).hexdigest(),"baseline_source_oid":baseline,"expected_agents":a.agents},indent=2)+"\n")
    print(json.dumps(summary,indent=2))

if __name__=="__main__": main()
