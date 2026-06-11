#ifndef compress_h
#define compress_h

void compress_error(char *error, char *out);
void compress_nlp(char **args, char *out);
void load_stopwords();
int is_stopword(char *word);
void add_candidate(char *word);
void load_candidates();
void save_candidates();

#endif
