# frozen_string_literal: true
require_relative 'test_helper'

# :skipped is done without effect and never compensated; :stuck parks a saga
# until #unstick.
class OutcomesTest < Minitest::Test
  include SagaHelpers

  def test_a_skipped_step_advances_and_is_not_compensated
    saga = Saga.new(4, ID)
    run_step(saga, :success)
    skipped = run_step(saga, :skipped)
    assert_equal Forward.new(2), saga.progress
    assert_equal 0b010, saga.skipped
    assert_equal Settlement.new(skipped, :skipped), saga.settled
    run_step(saga, :success)
    run_step(saga, :failure)
    assert_equal Compensating.new(2), saga.progress
    assert_equal [2, 0], [run_step(saga, :success).step, run_step(saga, :success).step]
    assert_equal :aborted, saga.progress
    assert_equal 0b010, saga.snapshot.skipped
  end

  def test_an_outcome_answers_only_its_kind
    saga = Saga.new(2, ID)
    forward = saga.next_action
    assert_unchanged(saga) { assert_equal :inapplicable, saga.resolve(forward, :stuck) }
    run_step(saga, :success)
    run_step(saga, :failure)
    undo = saga.next_action
    assert_unchanged(saga) { assert_equal :inapplicable, saga.resolve(undo, :skipped) }
  end

  def test_a_skipped_reply_keeps_stale_protection
    saga = Saga.new(3, ID)
    first = run_step(saga, :success)
    skipped = run_step(saga, :skipped)
    assert_unchanged(saga) do
      assert_equal :already_resolved, saga.resolve(skipped, :skipped)
      assert_equal :mismatch, saga.resolve(skipped, :success)
      assert_equal :stale, saga.resolve(first, :success)
    end
  end

  def test_a_stuck_compensation_parks_the_saga_and_survives_a_restart
    saga = Saga.new(2, ID)
    run_step(saga, :success)
    run_step(saga, :failure)
    undo = run_step(saga, :stuck)
    assert_equal Stuck.new(0), saga.progress
    assert_nil saga.pending
    assert_unchanged(saga) do
      assert_nil saga.next_action
      assert_equal :concluded, saga.cancel
      assert_equal :already_resolved, saga.resolve(undo, :stuck)
      assert_equal :mismatch, saga.resolve(undo, :success)
    end
    restored = Saga.restored(2, saga.snapshot, 0)
    assert_equal Stuck.new(0), restored.progress
    assert_equal saga.snapshot, restored.snapshot
    assert_nil restored.next_action
  end

  def test_a_snapshot_with_skipped_steps_restores
    saga = Saga.new(3, ID)
    run_step(saga, :success)
    run_step(saga, :skipped)
    snapshot = saga.snapshot
    assert_equal snapshot, Saga.restored(3, snapshot, 0).snapshot
    assert_raises(RangeError) { Saga.restored(3, snapshot.with(skipped: 0b100), 0) }
    assert_raises(TypeError) { Saga.restored(3, snapshot.with(skipped: nil), 0) }
  end
end
