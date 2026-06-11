import json
from pathlib import Path
from datetime import datetime

HISTORY_FILE=Path.home()/".local/share/aish/history.jsonl"





def save_message(role,content):
    HISTORY_FILE.parent.mkdir(
    parents=True,
    exist_ok=True
)
    with open(HISTORY_FILE,mode='a',encoding='utf-8') as f:
        entry={}
        entry["role"]=role
        entry["content"]=content
        entry["timestamp"]=datetime.now().isoformat()
        json.dump(entry,f)
        f.write("\n")


def load_history(n=20):
    if not HISTORY_FILE.exists():
        return []
    history=[]
    with open(HISTORY_FILE,mode='r',encoding='utf-8') as f:
        for line in f:
            history.append(json.loads(line))
    return history[-n:]



