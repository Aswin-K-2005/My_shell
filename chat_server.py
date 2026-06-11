import socket
import os
import sys
import requests
import json

sys.path.insert(0, os.path.expanduser("~/.config/aish"))
from retriever import retrieve

sock_path = "/tmp/aish_chat.sock"

if os.path.exists(sock_path):
    os.remove(sock_path)

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(sock_path)
server.listen(1)

while True:
    conn, _ = server.accept()
    conn.settimeout(5.0)
    
    # read all data until sentinel
    data = b""
    try:
        while True:
            chunk = conn.recv(4096)
            if not chunk:
                break
            data += chunk
            if b"__MSG_END__" in data:
                data = data.replace(b"__MSG_END__", b"")
                break
    except:
        pass
    data = data.decode()
    
    # retrieve relevant context from codebase
    try:
        results = retrieve(data, n_results=3)
        context = ""
        for filepath, chunk in results:
            filename = os.path.basename(filepath)
            context += f"\n--- From {filename} ---\n{chunk}\n"
    except:
        context = ""
    
    # build prompt based on content type
    if "Content:" in data and "Question:" in data:
        prompt = f"""You are Aish, an AI assistant for a developer.
Analyze the code and answer the question clearly and concisely.
Do not try to complete or fix the code unless asked.

{data}"""
    else:
        prompt = f"""You are Aish, an AI coding assistant with access to the developer's codebase.

Relevant code context:
{context}

Answer the question using the context above when relevant.
Be concise and technical.

User: {data}
Aish:"""
    
    # stream from ollama
    response = requests.post(
        "http://localhost:11434/api/generate",
        json={
            "model": "qwen3.5:9b",
            "prompt": prompt,
            "stream": True,
            "think": False,
            "options": {"temperature": 0.7}
        },
        stream=True
    )
    
    # send each token to C
    for line in response.iter_lines():
        if line:
            chunk = json.loads(line)
            token = chunk.get("response", "")
            if token:
                conn.send(token.encode())
    
    conn.send(b"__END__")
    conn.close()
