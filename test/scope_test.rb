# frozen_string_literal: true
require_relative 'test_helper'

# Attempts carry the scope of the saga that issued them, and a restart
# resumes the sequence instead of rewinding it.
class ScopeTest < Minitest::Test
  include SagaHelpers

  def test_attempts_carry_the_scope
    saga = Saga.with_scope(2, ID, 7)
    action = saga.next_action
    assert_equal [7, 1], [saga.scope, saga.issued]
    assert_equal [(7 << 48) | 1, 7], [action.attempt.raw, action.attempt.scope]
    assert_equal 0, Saga.new(1, ID).next_action.attempt.scope
    assert_equal 65_535, Saga.with_scope(1, ID, 65_535).scope
    refute_respond_to Saga, :native_scope_of
  end

  def test_scope_is_validated
    assert_raises(TypeError) { Saga.with_scope(1, ID, 1.0) }
    assert_raises(TypeError) { Saga.with_scope(1, ID, nil) }
    assert_raises(RangeError) { Saga.with_scope(1, ID, -1) }
    assert_raises(RangeError) { Saga.with_scope(1, ID, 65_536) }
  end

  def test_an_action_crosses_a_queue_as_plain_data
    saga = Saga.with_scope(1, ID, 5)
    issued = saga.next_action
    wire = [issued.key.saga.raw, issued.step, issued.kind, issued.attempt.raw]
    rebuilt = Action.new(ActionKey.new(SagaId.new(wire[0]), wire[1], wire[2]), AttemptId.new(wire[3]))
    assert_equal :resolved, saga.resolve(rebuilt, :success)
  end

  def test_a_restart_resumes_the_sequence
    saga = Saga.with_scope(3, ID, 4)
    first = run_step(saga, :success)
    lost = saga.next_action
    saved = saga.snapshot
    assert_equal Snapshot.new(ID, 4, Forward.new(1), 2, Settlement.new(first, :success)), saved

    restored = Saga.restored(3, saved, 4)
    assert_equal [saved, nil], [restored.snapshot, restored.pending]
    assert_unchanged(restored) do
      assert_equal :already_resolved, restored.resolve(first, :success)
      assert_equal :mismatch, restored.resolve(first, :failure)
      assert_equal :stale, restored.resolve(lost, :success)
    end
    reissued = restored.next_action
    assert_equal lost.key, reissued.key
    assert_equal (4 << 48) | 3, reissued.attempt.raw
    assert_equal :stale, restored.resolve(lost, :success)
    assert_equal :resolved, restored.resolve(reissued, :success)
  end

  def test_every_reachable_snapshot_restores
    saga = Saga.new(2, ID)
    check = lambda do
      copy = Saga.restored(2, saga.snapshot, 0)
      assert_equal saga.snapshot, copy.snapshot
    end
    check.call
    run_step(saga, :success)
    check.call
    run_step(saga, :failure)
    check.call
    run_step(saga, :failure)
    check.call
    run_step(saga, :success)
    check.call
    assert_equal :aborted, saga.progress
    done = Saga.new(1, ID)
    run_step(done, :success)
    assert_equal :complete, Saga.restored(1, done.snapshot, 0).progress
    assert_equal :aborted, Saga.restored(1, Snapshot.new(ID, 0, :aborted, 0, nil), 0).progress
  end

  def test_unsound_saved_state_raises
    saga = Saga.with_scope(2, ID, 4)
    first = run_step(saga, :success)
    saved = saga.snapshot
    unsound = [
      [2, saved, 5], [1, saved, 4],
      [2, saved.with(progress: Forward.new(0)), 4],
      [2, saved.with(progress: Forward.new(2)), 4], [2, saved.with(progress: :complete), 4],
      [2, saved.with(settled: nil), 4],
      [2, saved.with(settled: Settlement.new(first, :failure)), 4]
    ]
    unsound.each do |steps, snapshot, scope|
      error = assert_raises(RangeError, snapshot.inspect) { Saga.restored(steps, snapshot, scope) }
      assert_match(/saved state/, error.message)
    end
    assert_instance_of Saga, Saga.restored(2, saved, 4)
    assert_instance_of Saga, Saga.restored(2, saved.with(progress: Compensating.new(0)), 4)
  end

  # `Snapshot::new` refuses what no saga of any step count can have saved:
  # such a snapshot is never built at all.
  def test_a_snapshot_no_saga_of_any_steps_can_save_does_not_exist
    saga = Saga.with_scope(2, ID, 4)
    first = run_step(saga, :success)
    saved = saga.snapshot
    [
      -> { saved.with(issued: 0) }, -> { saved.with(issued: 2**48) },
      -> { saved.with(id: SagaId.new(8)) }, -> { saved.with(scope: 5) },
      -> { saved.with(settled: Settlement.new(first.with(attempt: AttemptId.new(1)), :success)) },
      -> { Snapshot.new(ID, 0, Forward.new(1), 0, Settlement.new(action(0, :forward, 1), :success)) }
    ].each { |build| assert_raises(RangeError) { build.call } }
    # Not restorable for two steps, but a value in its own right.
    assert_equal Forward.new(0), saved.with(progress: Forward.new(0)).progress
  end

  # The check is exact: these were accepted while the floor followed the
  # progress instead of the settlement.
  def test_a_snapshot_no_saga_can_produce_raises
    undone = lambda do |issued, attempt|
      Snapshot.new(ID, 0, :aborted, issued, Settlement.new(action(0, :compensate, attempt), :success))
    end
    done = Settlement.new(action(0, :forward, 1), :success)
    unsound = [
      # Attempt 1 is the forward try of step 0, never its compensation.
      [2, undone.call(1, 1)],
      # Nothing is issued after a settlement that finished the saga.
      [2, undone.call(3, 2)], [1, Snapshot.new(ID, 0, :complete, 2, done)],
      [0, Snapshot.new(ID, 0, :complete, 1, nil)]
    ]
    unsound.each do |steps, snapshot|
      error = assert_raises(RangeError, snapshot.inspect) { Saga.restored(steps, snapshot, 0) }
      assert_match(/saved state/, error.message)
    end
    sound = [
      [2, undone.call(2, 2)], [2, undone.call(9, 9)], [1, Snapshot.new(ID, 0, :complete, 1, done)],
      [0, Snapshot.new(ID, 0, :complete, 0, nil)],
      # A cancel settles nothing: attempts lost to restarts may precede it.
      [2, Snapshot.new(ID, 0, :aborted, 5, nil)]
    ]
    sound.each { |steps, snapshot| assert_equal snapshot, Saga.restored(steps, snapshot, 0).snapshot }
  end

  def test_restored_validates_its_input
    saved = Saga.new(2, ID).snapshot
    assert_instance_of Saga, Saga.restored(2, saved, 0)
    assert_raises(TypeError) { Saga.restored(2, nil, 0) }
    assert_raises(TypeError) { Saga.restored(2, saved.to_h, 0) }
    assert_raises(TypeError) { Saga.restored(2, saved.with(id: 7), 0) }
    assert_raises(TypeError) { Saga.restored(2, saved.with(scope: 0.0), 0) }
    assert_raises(TypeError) { Saga.restored(2, saved.with(issued: nil), 0) }
    assert_raises(TypeError) { Saga.restored(2, saved.with(progress: 0), 0) }
    assert_raises(TypeError) { Saga.restored(2, saved.with(progress: Forward.new(nil)), 0) }
    error = assert_raises(TypeError) { Saga.restored(2, saved.with(settled: [1, :success]), 0) }
    assert_equal 'expected a SagaMachines::Settlement or nil', error.message
    [0, Forward, action(0, :forward, 1)].each do |progress|
      error = assert_raises(TypeError) { Snapshot.new(ID, 0, progress, 0, nil) }
      assert_equal 'expected a Forward, a Compensating, :complete or :aborted', error.message
    end
    assert_raises(TypeError) { Saga.restored(2, saved, '0') }
    assert_raises(RangeError) { Saga.restored(2, saved.with(progress: :forward), 0) }
    assert_raises(RangeError) { Saga.restored(2, saved.with(progress: :paused), 0) }
    assert_raises(RangeError) { Settlement.new(action(0, :forward, 1), :timeout) }
    assert_raises(TypeError) { Settlement.new(action(0, :forward, 1), 'success') }
    assert_raises(TypeError) { Settlement.new(nil, :success) }
    assert_raises(RangeError) { Saga.restored(2, saved.with(scope: 65_536), 0) }
    assert_raises(RangeError) { Saga.restored(2, saved.with(issued: -1), 0) }
    assert_raises(RangeError) { Saga.restored(5, saved, 0) }
  end
end
