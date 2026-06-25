import socket
import os
import sys
import requests
import json
from context.history import save_message,load_history

sys.path.insert(0, os.path.expanduser("~/.config/aish"))
from retriever import (
    retrieve,
    retrieve_from_file,
    extract_filename,
)

sock_path = "/tmp/aish_chat.sock"

if os.path.exists(sock_path):
    os.remove(sock_path)

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(sock_path)
server.listen(1)



while True:
    conn, _ = server.accept()
    conn.settimeout(5.0)
    
    # read all data until sentinel]
    history=load_history(10)
    history_context=""
    for msg in history:
        history_context+=(
                f"{msg['role']}:" 
                f"{msg['content']}\n" 
                f"{msg['timestamp']}\n" 
            )
            
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
    print("INPUT LEN:", len(data))
    save_message("user",data)
    
    # retrieve relevant context from codebase
    try:
        filename = extract_filename(data)

        if filename:
            print(f"FILE DETECTED: {filename}")

            results = retrieve_from_file(
            filename,
            data,
            n_results=5
            )
        else:
            results = retrieve(
                data,
                 n_results=3
            )

        print("RETRIEVED FILES:")

        context = ""

        for filepath, chunk in results:
            print(filepath)

            short_name = os.path.basename(filepath)

            context += (
                f"\n--- From {short_name} ---\n"
                f"{chunk}\n"
        )

    except Exception as e:
        print("RETRIEVAL ERROR:", e)
        context = ""    
    # build prompt based on content type
    if "Content:" in data and "Question:" in data:
        prompt = f"""You are Aish, an AI assistant for a developer.
Analyze the code and answer the question clearly and concisely.
Do not try to complete or fix the code unless asked.
 
Recent conversation:
{history_context}
 
 Query:
{data}"""
    else:
        prompt = f"""You are Aish, an AI coding assistant with access to the developer's codebase.
Recent Conversation:
{history_context} 
 
Relevant code context:
{context}

Answer the question using the context above when relevant.
Be concise and technical.

User: {data}
Aish:"""
    print("PROMPT LEN:", len(prompt))
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
    full_response="" 
    # send each token to C
    for line in response.iter_lines():
        if line:
            chunk = json.loads(line)
            token = chunk.get("response", "")
            if token:
                full_response+=token
                print("TOKEN:", repr(token), flush=True)
                conn.send(token.encode())
       
    save_message("assistant",full_response)
    print("FULL RESPONSE:", repr(full_response))
    print("FULL RESPONSE LEN:", len(full_response))


    conn.send(b"__END__")
    conn.close()
