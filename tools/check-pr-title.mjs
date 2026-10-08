import assert from 'node:assert/strict';

const conventional = /^(feat|fix|perf|refactor|docs|test|build|ci|chore)(\([a-z0-9][a-z0-9-]*\))?!?: .+$/;

if (process.argv[2] === '--self-test') {
  for (const title of ['feat(field): add query', 'fix!: correct sign', 'chore(main): release 0.2.0']) {
    assert.match(title, conventional);
  }
  for (const title of ['Add query', 'feat: ', 'FEAT: add query', 'fix(scope) add query']) {
    assert.doesNotMatch(title, conventional);
  }
} else if (!conventional.test(process.env.PR_TITLE ?? '')) {
  console.error('PR title must follow Conventional Commits: type(scope)!: summary');
  process.exitCode = 1;
}
