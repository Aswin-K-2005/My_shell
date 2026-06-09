import chromadb
import requests
import os

# setup ChromaDB
chroma_path = os.path.expanduser("~/.config/aish/chromadb")
client = chromadb.PersistentClient(path=chroma_path)
collection = client.get_or_create_collection("error_cache")

def get_embedding(text):
    response = requests.post(
        "http://localhost:11434/api/embeddings",
        json={"model": "nomic-embed-text", "prompt": text}
    )
    return response.json()["embedding"]

def search_cache(error, threshold=0.85):
    embedding = get_embedding(error)
    results = collection.query(
        query_embeddings=[embedding],
        n_results=1
    )
    distances = results.get("distances", [[]])
    documents = results.get("documents", [[]])
    
    # log to file
    with open("/tmp/aish_debug.log", "a") as f:
        f.write(f"error: {error[:50]}\n")
        f.write(f"distances: {distances}\n\n")
    
    if distances and distances[0] and distances[0][0] < threshold:
        return documents[0][0]
    return None
def store_cache(error, explanation):
    embedding = get_embedding(error)
    collection.add(
        embeddings=[embedding],
        documents=[explanation],
        ids=[str(hash(error))]
    )
