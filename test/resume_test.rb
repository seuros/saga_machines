# frozen_string_literal: true
require_relative 'test_helper'

# #unstick retries a parked compensation; #reconcile settles work that was in
# flight when the process died.
class ResumeTest < Minitest::Test
  include SagaHelpers

  # Two steps done, the third failed, the compensation of step 1 stuck.
  def parked
    saga = Saga.new(3, ID)
    run_step(saga, :success)
    run_step(saga, :success)
    run_step(saga, :failure)
    [saga, run_step(saga, :stuck)]
  end

  def test_unstick_resumes_the_stuck_compensation_under_a_fresh_attempt
    saga, undo = parked
    assert_equal Stuck.new(1), saga.progress
    issued = saga.issued
    assert_equal :unstuck, saga.unstick
    assert_equal Compensating.new(1), saga.progress
    assert_equal issued, saga.issued
    assert_nil saga.pending
    again = saga.next_action
    assert_equal undo.key, again.key
    refute_equal undo.attempt, again.attempt
    assert_equal :resolved, saga.resolve(again, :success)
    run_step(saga, :success)
    assert_equal :aborted, saga.progress
  end

  def test_unstick_refuses_a_saga_that_is_not_stuck
    fresh = Saga.new(2, ID)
    assert_unchanged(fresh) { assert_equal :not_stuck, fresh.unstick }
    fresh.next_action
    assert_unchanged(fresh) { assert_equal :not_stuck, fresh.unstick }
    saga, = parked
    assert_equal :unstuck, saga.unstick
    assert_unchanged(saga) { assert_equal :not_stuck, saga.unstick }
  end

  def test_the_stuck_attempt_never_resolves_the_retry
    saga, undo = parked
    saga.unstick
    again = saga.next_action
    assert_unchanged(saga) do
      assert_equal :already_resolved, saga.resolve(undo, :stuck)
      assert_equal :mismatch, saga.resolve(undo, :success)
    end
    assert_equal again, saga.pending
    assert_equal :resolved, saga.resolve(again, :failure)
    assert_unchanged(saga) { assert_equal :stale, saga.resolve(undo, :stuck) }
  end

  def test_a_stuck_snapshot_restores_and_is_retried_later
    saga, = parked
    restored = Saga.restored(3, saga.snapshot, 0)
    assert_equal Stuck.new(1), restored.progress
    assert_equal :unstuck, restored.unstick
    resumed = Saga.restored(3, restored.snapshot, 0)
    assert_equal Compensating.new(1), resumed.progress
    run_step(resumed, :success)
    run_step(resumed, :success)
    assert_equal :aborted, resumed.progress
  end

  def test_reconcile_settles_the_step_the_saga_is_on
    saga = Saga.new(3, ID)
    settled = saga.reconcile(0, :forward, :success)
    assert_equal ActionKey.new(ID, 0, :forward), settled.key
    assert_equal Forward.new(1), saga.progress
    assert_nil saga.pending
    assert_equal Settlement.new(settled, :success), saga.settled
    assert_unchanged(saga) { assert_equal :already_resolved, saga.resolve(settled, :success) }
    saga.reconcile(1, :forward, :skipped)
    assert_equal 0b010, saga.skipped
    saga.reconcile(2, :forward, :failure)
    assert_equal Compensating.new(0), saga.progress
    saga.reconcile(0, :compensate, :stuck)
    assert_equal Stuck.new(0), saga.progress
  end

  def test_reconcile_settles_the_pending_action_when_there_is_one
    saga = Saga.new(2, ID)
    pending = saga.next_action
    assert_equal pending, saga.reconcile(0, :forward, :success)
    assert_equal 1, saga.issued
    assert_nil saga.pending
  end

  def test_a_kill_mid_flight_is_reconciled_to_completion
    saga = Saga.with_scope(2, ID, 3)
    run_step(saga, :success)
    lost = saga.next_action
    restored = Saga.restored(2, saga.snapshot, 3)
    assert_nil restored.pending
    restored.reconcile(lost.step, lost.kind, :success)
    assert_equal :complete, restored.progress
    assert_unchanged(restored) { assert_equal :already_resolved, restored.resolve(restored.settled.action, :success) }
    assert_unchanged(restored) { assert_equal :stale, restored.resolve(lost, :success) }
  end

  def test_a_kill_mid_flight_is_reconciled_to_a_full_compensation
    saga = Saga.new(3, ID)
    run_step(saga, :success)
    run_step(saga, :success)
    saga.next_action
    restored = Saga.restored(3, saga.snapshot, 0)
    restored.reconcile(2, :forward, :failure)
    assert_equal Compensating.new(1), restored.progress
    restored.reconcile(1, :compensate, :success)
    restored.reconcile(0, :compensate, :success)
    assert_equal :aborted, restored.progress
  end

  def test_reconcile_refuses_what_the_saga_cannot_be_waiting_on
    saga = Saga.new(3, ID)
    run_step(saga, :success)
    assert_unchanged(saga) do
      assert_equal :mismatch, saga.reconcile(0, :forward, :success)
      assert_equal :mismatch, saga.reconcile(2, :forward, :success)
      assert_equal :mismatch, saga.reconcile(1, :compensate, :success)
      assert_equal :inapplicable, saga.reconcile(1, :forward, :stuck)
      assert_equal :inapplicable, saga.reconcile(1, :compensate, :skipped)
    end
    done = Saga.new(1, ID)
    run_step(done, :success)
    stuck, = parked
    assert_unchanged(done) { assert_equal :concluded, done.reconcile(0, :forward, :success) }
    assert_unchanged(stuck) { assert_equal :concluded, stuck.reconcile(1, :compensate, :success) }
  end

  def test_reconcile_validates_its_arguments
    saga = Saga.new(2, ID)
    assert_unchanged(saga) do
      assert_raises(TypeError) { saga.reconcile('0', :forward, :success) }
      assert_raises(RangeError) { saga.reconcile(0, :sideways, :success) }
      assert_raises(RangeError) { saga.reconcile(0, :forward, :unknown) }
    end
  end
end
