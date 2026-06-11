from chunker import chunk_file

chunks = chunk_file("new.c")
print(f"Total chunks: {len(chunks)}")
for i, chunk in enumerate(chunks[:3]):
    print(f"\n--- Chunk {i+1} ---")
    print(chunk[:300])
