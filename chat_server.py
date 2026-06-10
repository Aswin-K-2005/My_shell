import socket
import os
import requests
import json

sock_path = "/tmp/aish_chat.sock"

if os.path.exists(sock_path):
    os.remove(sock_path)

server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
server.bind(sock_path)
server.listen(1)

while True:
    conn, _ = server.accept()
    
    # read all data until sentinel
    data = b""
    while True:
        chunk = conn.recv(4096)
        if not chunk:
            break
        data += chunk
        if b"__MSG_END__" in data:
            data = data.replace(b"__MSG_END__", b"")
            break
    data = data.decode()
    
    # build prompt based on content
    if "Content:" in data and "Question:" in data:
        prompt = f"""You are Aish, an AI assistant for a developer.
Analyze the code and answer the question clearly and concisely.
Do not try to complete or fix the code unless asked.

{data}"""
    else:
        prompt = f"""You are Aish, a helpful AI assistant for developers.
Answer concisely and technically.

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
