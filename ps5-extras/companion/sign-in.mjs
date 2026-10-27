// Sign-in orchestration is separate from HTTP and can be exercised without an account.
export async function signIn({ backend, command, signal, report }) {
  const checkCancelled = () => { if (signal.aborted) throw Error('Sign-in cancelled.'); };
  checkCancelled();
  report({ stage: 'checking', message: 'Checking your saved Microsoft sign-in…' });
  let existing;
  try { existing = await backend.status(true); } catch { /* A missing account must still allow login. */ }
  checkCancelled();
  if (!existing?.sessionVerified) {
    report({ stage: 'signing-in', message: 'Complete Microsoft sign-in in the Xodus window on this Mac. This expires after ten minutes; you can cancel and retry.' });
    try { await command(['login'], { timeout: 10 * 60000, onLine() {}, signal }); }
    catch (error) {
      checkCancelled();
      throw Error(error.message.includes('timed out')
        ? 'Sign-in expired after ten minutes. Choose Sign in to try again.'
        : 'Microsoft sign-in did not complete. The window may have been closed. Choose Sign in to try again.');
    }
    checkCancelled();
    backend.library.reset();
  }
  report({ stage: 'verifying', message: 'Verifying the Microsoft session and PC Game Pass membership…' });
  const status = await backend.status(true);
  checkCancelled();
  if (!status.sessionVerified) throw Error('The Microsoft session could not be verified. Check your connection and try signing in again.');
  report({ stage: 'signed-in', message: status.pcGamePass
    ? 'Microsoft sign-in and PC Game Pass verified on this Mac. The console account is connected separately.'
    : 'Microsoft sign-in verified on this Mac. An active PC Game Pass or Ultimate subscription could not be verified; downloads remain unavailable.' });
}
