# retriever.py
import os
import requests
import chromadb

chroma_path = os.path.expanduser("~/.config/aish/chromadb")
client = chromadb.PersistentClient(path=chroma_path)
collection = client.get_or_create_collection("codebase")

def get_embedding(text):
    response = requests.post(
        "http://localhost:11434/api/embeddings",
        json={"model": "nomic-embed-text", "prompt": text}
    )
    return response.json()["embedding"]

def retrieve(query, n_results=3):
    embedding = get_embedding(query)
    results = collection.query(
        query_embeddings=[embedding],
        n_results=n_results
    )
    chunks = results.get("documents", [[]])[0]
    files = [m["file"] for m in results.get("metadatas", [[]])[0]]
    return list(zip(files, chunks))

if __name__ == "__main__":
    # test
    results = retrieve("lsh_find_pipe lsh_split_pipe pipefd")
    for filepath, chunk in results:
        print(f"\n--- From {filepath} ---")
        print(chunk[:300])
