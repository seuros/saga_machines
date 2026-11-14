# frozen_string_literal: true
require_relative 'test_helper'

class SagaTest < Minitest::Test
  include SagaHelpers

  def test_forward_completion
    saga = Saga.new(8, ID)
    assert_equal [8, ID, 0, 0], [saga.steps, saga.id, saga.scope, saga.issued]
    assert_equal [Forward.new(0), nil, nil], [saga.progress, saga.pending, saga.settled]
    8.times do |step|
      assert_equal Forward.new(step), saga.progress
      action = saga.next_action
      assert_equal action(step, :forward, step + 1), action
      assert_equal [step, :forward], [action.step, action.kind]
      assert_equal action, saga.pending
      assert_equal :resolved, saga.resolve(action, :success)
      assert_equal Settlement.new(action, :success), saga.settled
      assert_nil saga.pending
    end
    assert_equal :complete, saga.progress
    assert_nil saga.next_action
    assert_equal 8, saga.issued
  end

  def test_reverse_compensation_and_retry
    saga = Saga.new(4, ID)
    3.times { run_step(saga, :success) }
    failed = run_step(saga, :failure)
    assert_equal [3, :forward], [failed.step, failed.kind]
    assert_equal Compensating.new(2), saga.progress
    undo = run_step(saga, :failure)
    assert_equal [2, :compensate], [undo.step, undo.kind]
    assert_equal Compensating.new(2), saga.progress
    again = saga.next_action
    assert_equal undo.key, again.key
    refute_equal undo.attempt, again.attempt
    assert_equal :resolved, saga.resolve(again, :success)
    [1, 0].each { |step| assert_equal step, run_step(saga, :success).step }
    assert_equal :aborted, saga.progress
    assert_nil saga.next_action
  end

  def test_next_action_repeats_the_pending_action
    saga = Saga.new(2, ID)
    action = saga.next_action
    assert_unchanged(saga) { assert_equal action, saga.next_action }
    assert_equal 1, saga.issued
  end

  def test_cancel_and_its_repeat
    fresh = Saga.new(1, ID)
    assert_equal :cancelled, fresh.cancel
    assert_equal :aborted, fresh.progress
    assert_unchanged(fresh) { assert_equal :already_unwinding, fresh.cancel }

    saga = Saga.new(2, ID)
    run_step(saga, :success)
    assert_equal :cancelled, saga.cancel
    assert_equal Compensating.new(0), saga.progress
    assert_unchanged(saga) { assert_equal :already_unwinding, saga.cancel }
    undo = saga.next_action
    assert_unchanged(saga) { assert_equal :already_unwinding, saga.cancel }
    assert_equal :resolved, saga.resolve(undo, :success)
    assert_equal :aborted, saga.progress
  end

  def test_a_repeated_resolve_of_the_last_settlement_is_honoured
    saga = Saga.new(3, ID)
    first = run_step(saga, :success)
    assert_unchanged(saga) { assert_equal :already_resolved, saga.resolve(first, :success) }
    second = saga.next_action
    assert_unchanged(saga) { assert_equal :already_resolved, saga.resolve(first, :success) }
    assert_equal :resolved, saga.resolve(second, :success)
    assert_equal :cancelled, saga.cancel
    assert_unchanged(saga) do
      assert_equal :already_resolved, saga.resolve(second, :success)
      assert_equal :stale, saga.resolve(first, :success)
    end
  end

  def test_every_compiled_step_count
    assert_equal [0, 1, 2, 3, 4, 8, 16], STEPS
    assert_predicate STEPS, :frozen?
    STEPS.each do |steps|
      saga = Saga.new(steps, ID)
      assert_equal steps, saga.steps
      steps.times { |step| assert_equal step, run_step(saga, :success).step }
      assert_nil saga.next_action
      assert_equal :complete, saga.progress
      assert_equal :concluded, saga.cancel
    end
    [5, 7, 17, 2**64 - 1].each { |steps| assert_raises(RangeError) { Saga.new(steps, ID) } }
  end

  def test_identities
    id = SagaId.from_bytes('manifest-42')
    assert_equal id, SagaId.from_bytes('manifest-42')
    refute_equal id, SagaId.from_bytes('manifest-43')
    assert_equal SagaId.new(0xcbf29ce484222325), SagaId.from_bytes('')
    assert_equal SagaId.from_bytes("\xFF".b), SagaId.from_bytes("\xFF".dup.force_encoding('UTF-8'))
    assert_raises(TypeError) { SagaId.from_bytes(42) }
    assert_equal id, Saga.new(1, id).next_action.key.saga
    refute_respond_to Saga, :native_from_bytes
    refute_respond_to Saga, :native_with_scope
    refute_respond_to Saga.new(1, ID), :native_snapshot
  end

  def test_validation
    [1.0, nil, '1', Float::NAN].each do |bad|
      assert_raises(TypeError) { Saga.new(bad, ID) }
      assert_raises(TypeError) { Saga.new(1, SagaId.new(bad)) }
      assert_raises(TypeError) { Saga.new(1, bad) }
    end
    assert_raises(TypeError) { Saga.new(1, 7) }
    [-1, 2**64].each { |bad| assert_raises(RangeError) { Saga.new(1, SagaId.new(bad)) } }
    assert_raises(RangeError) { Saga.new(-1, ID) }
    assert_raises(ArgumentError) { Saga.new(1, ID, steps: 1) }
    assert_raises(TypeError) { Saga.allocate }
    saga = Saga.new(1, SagaId.new(2**64 - 1))
    action = saga.next_action
    assert_unchanged(saga) do
      assert_raises(RangeError) { saga.resolve(action, :timeout) }
      assert_raises(TypeError) { saga.resolve(action, 'success') }
      assert_raises(TypeError) { saga.resolve(nil, :success) }
      assert_raises(TypeError) { saga.resolve(action.key, :success) }
      assert_raises(TypeError) { saga.resolve(action.with(attempt: 1), :success) }
      assert_raises(TypeError) { saga.resolve(action.with(key: [2**64 - 1, 0, :forward]), :success) }
      assert_raises(TypeError) { saga.resolve(action.with(key: action.key.with(step: 0.0)), :success) }
      assert_raises(TypeError) { saga.resolve(action.with(key: action.key.with(kind: 'forward')), :success) }
      assert_raises(RangeError) { saga.resolve(action.with(key: action.key.with(kind: :sideways)), :success) }
      assert_raises(RangeError) { saga.resolve(action.with(attempt: AttemptId.new(-1)), :success) }
    end
  end

  def test_freeze
    saga = Saga.new(1, ID)
    action = saga.next_action
    saga.freeze
    assert_raises(FrozenError) { saga.next_action }
    assert_raises(FrozenError) { saga.cancel }
    assert_raises(FrozenError) { saga.resolve(action, :success) }
    assert_equal [Forward.new(0), action, 1], [saga.progress, saga.pending, saga.issued]
  end

  # Frozen is checked before any argument is read: a frozen saga handed a
  # malformed argument raises FrozenError, as Ruby's own core methods do.
  def test_frozen_is_checked_before_the_arguments
    saga = Saga.new(1, ID)
    action = saga.next_action
    saga.freeze
    assert_raises(FrozenError) { saga.resolve(nil, :success) }
    assert_raises(FrozenError) { saga.resolve(action, 'success') }
    assert_raises(FrozenError) { saga.resolve(action, :unknown) }
  end

  def test_gc_stress
    previous = GC.stress
    GC.stress = true
    saga = Saga.new(1, ID)
    action = saga.next_action
    GC.compact
    assert_equal action, saga.next_action
    assert_equal :resolved, saga.resolve(action, :success)
    assert_equal :mismatch, saga.resolve(action, :failure)
    assert_equal :complete, saga.snapshot.progress
  ensure
    GC.stress = previous
  end
end
