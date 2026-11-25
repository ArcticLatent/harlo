## 2026-11-24 23:29 · cold-cache · openai/gpt-6.2-sol

Revision 1c720bad with uncommitted changes; paid by a ChatGPT plan (the dollars are what a key would pay); rest before each prompt: none (every prompt counts as rested); prices (2, 2.5, 0.1, 10). First run of the cold-cache clearing.

| arm | changes work (textwrap, argparse, difflib) | steps | reads | cleared | prompt tokens | output | $ billed | $ if cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| off | +++ |    25 |    16 |       0 |  1,215,257 |   1,643 |   1.817 |   2.039 |
|  on | +++ |    28 |    16 |       5 |  1,040,508 |   1,723 |   1.857 |   1.916 |
| off | +++ |    25 |    16 |       0 |  1,224,477 |   1,645 |   2.169 |   2.423 |
|  on | +++ |    25 |    15 |       5 |    909,874 |   1,608 |   1.656 |   1.716 |

Per prompt (steps / reads / prompt tokens / $ if cold):

- off: 13 / 11 / 423,696 / 0.827; 6 / 3 / 391,171 / 0.424; 3 / 1 / 199,405 / 0.351; 3 / 1 / 200,985 / 0.437
- on: 15 / 13 / 527,024 / 0.966; 6 / 1 / 232,305 / 0.415; 3 / 1 / 118,962 / 0.188; 4 / 1 / 162,217 / 0.346
- off: 15 / 13 / 522,524 / 1.006; 4 / 1 / 274,524 / 0.488; 3 / 1 / 212,685 / 0.462; 3 / 1 / 214,744 / 0.467
- on: 13 / 11 / 423,729 / 0.746; 5 / 2 / 196,843 / 0.417; 3 / 1 / 122,513 / 0.196; 4 / 1 / 166,789 / 0.356

## 2026-11-25 01:15 · cold-cache · openai/gpt-6.2-sol

Revision 1c720bad with uncommitted changes; paid by a ChatGPT plan (the dollars are what a key would pay); rest before each prompt: none (every prompt counts as rested); prices (2, 2.5, 0.1, 10). Second run: requests carry prompt_cache_key, cache writes counted apart.

| arm | changes work (textwrap, argparse, difflib) | steps | reads | cleared | prompt tokens | from cache | output | $ billed | $ if cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| off | +++ |    27 |    18 |       0 |  1,287,843 |  30% |   1,681 |   1.858 |   2.076 |
|  on | +++ |    24 |    17 |       4 |    831,786 |  12% |   1,696 |   1.475 |   1.626 |
| off | +++ |    28 |    19 |       0 |  1,303,892 |  25% |   1,832 |   1.990 |   2.335 |
|  on | ++- |    25 |    16 |       7 |    824,663 |   5% |   1,518 |   1.575 |   1.682 |

Per prompt (steps / reads / prompt tokens / from cache / $ if cold):

- off: 16 / 14 / 563,689 / 8% / 1.047; 5 / 2 / 324,866 / 49% / 0.380; 3 / 1 / 198,692 / 56% / 0.338; 3 / 1 / 200,596 / 32% / 0.312
- on: 12 / 11 / 279,153 / 5% / 0.531; 6 / 4 / 262,005 / 0% / 0.553; 3 / 1 / 144,389 / 0% / 0.314; 3 / 1 / 146,239 / 62% / 0.227
- off: 17 / 15 / 580,737 / 3% / 1.137; 5 / 2 / 324,954 / 35% / 0.580; 3 / 1 / 198,319 / 65% / 0.307; 3 / 1 / 199,882 / 36% / 0.310
- on: 15 / 13 / 521,147 / 2% / 1.031; 6 / 2 / 187,512 / 0% / 0.398; 3 / 1 / 116,004 / 31% / 0.253; 1 / 0 / 0 / 0% / 0.000
- on: difflib_why AttributeError: 'SequenceMatcher' object has no attribute 'longest'
- on: a step failed: {'message': "The model's server reported an error: Our servers are currently overloaded. Please try again later.", 'type': 'provider'}
- on: a step failed: {'message': "The model's server reported an error: Our servers are currently overloaded. Please try again later.", 'type': 'provider'}
- on: a step failed: {'message': "The model's server reported an error: Our servers are currently overloaded. Please try again later.", 'type': 'provider'}

## 2026-11-25 12:50 · cold-cache · openai/gpt-6.2-sol

Revision 154f86f2 with uncommitted changes; paid by a ChatGPT plan (the dollars are what a key would pay); rest before each prompt: none (every prompt counts as rested); prices (2, 2.5, 0.1, 10); the on arm turns on: outline; task: jobs (textwrap, argparse, difflib, longest). First run of the four-jobs task for outline (tab line numbers in both arms).

| arm | work done | steps | reads | cleared | prompt tokens | from cache | output | $ billed | $ if cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| off | ++++ |    25 |     9 |       0 |     61,386 |   0% |   1,444 |   0.137 |   0.138 |
|  on | ++++ |    20 |     6 |       0 |     27,227 |   0% |   1,219 |   0.067 |   0.067 |
| off | ++++ |    23 |     7 |       0 |     58,337 |   0% |   1,343 |   0.130 |   0.131 |
|  on | ++++ |    20 |     7 |       0 |     29,692 |   0% |   1,169 |   0.071 |   0.072 |

Per prompt (steps / reads / prompt tokens / from cache / $ if cold):

- off: 6 / 2 / 10,747 / 0% / 0.026; 6 / 2 / 8,056 / 0% / 0.019; 6 / 2 / 11,324 / 0% / 0.026; 7 / 3 / 31,259 / 0% / 0.066
- on: 6 / 3 / 7,128 / 0% / 0.020; 5 / 1 / 4,722 / 0% / 0.012; 5 / 1 / 6,983 / 0% / 0.017; 4 / 1 / 8,394 / 0% / 0.019
- off: 5 / 1 / 18,104 / 0% / 0.041; 6 / 2 / 8,438 / 0% / 0.020; 5 / 1 / 6,318 / 0% / 0.016; 7 / 3 / 25,477 / 0% / 0.055
- on: 6 / 3 / 7,311 / 0% / 0.020; 5 / 1 / 4,735 / 0% / 0.012; 5 / 2 / 9,256 / 0% / 0.021; 4 / 1 / 8,390 / 0% / 0.019

Tool calls:

- off: edit 4, glob 5, grep 4, read 9, write 1
- on: edit 4, glob 5, grep 2, outline 2, read 4, write 1
- off: edit 4, glob 5, grep 4, read 7, write 1
- on: edit 4, glob 5, grep 1, outline 3, read 4, write 1

## 2026-11-25 12:54 · cold-cache · openai/gpt-6.2-sol

Revision 154f86f2 with uncommitted changes; paid by a ChatGPT plan (the dollars are what a key would pay); rest before each prompt: none (every prompt counts as rested); prices (2, 2.5, 0.1, 10); the on arm turns on: files; task: jobs (textwrap, argparse, difflib, longest). First run of the four-jobs task for files (tab line numbers in both arms).

| arm | work done | steps | reads | cleared | prompt tokens | from cache | output | $ billed | $ if cold |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| off | ++++ |    30 |     7 |       0 |    105,052 |   3% |   1,527 |   0.218 |   0.219 |
|  on | ++++ |    24 |     4 |       0 |     42,489 |   0% |   1,464 |   0.100 |   0.101 |
| off | ++++ |    30 |     7 |       0 |     99,453 |   0% |   1,557 |   0.214 |   0.216 |
|  on | ++++ |    22 |     3 |       0 |     34,562 |   0% |   1,463 |   0.084 |   0.085 |

Per prompt (steps / reads / prompt tokens / from cache / $ if cold):

- off: 7 / 1 / 32,570 / 0% / 0.070; 6 / 1 / 8,685 / 0% / 0.020; 8 / 2 / 21,759 / 17% / 0.040; 9 / 3 / 42,038 / 0% / 0.088
- on: 8 / 2 / 13,786 / 0% / 0.033; 5 / 1 / 7,518 / 0% / 0.018; 6 / 1 / 12,054 / 0% / 0.028; 5 / 0 / 9,131 / 0% / 0.023
- off: 8 / 1 / 38,892 / 0% / 0.083; 6 / 1 / 8,862 / 0% / 0.021; 7 / 2 / 17,776 / 0% / 0.039; 9 / 3 / 33,923 / 0% / 0.072
- on: 7 / 1 / 9,806 / 0% / 0.025; 5 / 1 / 6,663 / 0% / 0.016; 5 / 1 / 8,904 / 0% / 0.021; 5 / 0 / 9,189 / 0% / 0.023

Tool calls:

- off: edit 4, execute 7, glob 4, grep 4, read 7, write 1
- on: edit 4, execute 3, glob 4, grep 4, read 4, write 1
- off: edit 4, execute 6, glob 5, grep 4, read 7, write 1
- on: edit 4, execute 1, glob 4, grep 5, read 3, write 1

