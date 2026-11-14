# frozen_string_literal: true
require_relative 'test_helper'

# Every refusal is a symbol, never an exception, and changes nothing.
class RefusalsTest < Minitest::Test
  include SagaHelpers

  LAST = 2**48 - 1

  def test_exhaustion_is_a_refusal_and_not_the_terminal_answer
    saved = Snapshot.new(ID, 3, Forward.new(0), LAST - 1, nil)
    saga = Saga.restored(1, saved, 3)
    last = saga.next_action
    assert_equal (3 << 48) | LAST, last.attempt.raw
    assert_unchanged(saga) { assert_equal last, saga.next_action }
    assert_equal :resolved, saga.resolve(last, :failure)
    assert_nil saga.next_action

    stuck = Saga.restored(2, saved, 3)
    run_step(stuck, :success)
    assert_equal Forward.new(1), stuck.progress
    assert_unchanged(stuck) { assert_equal :exhausted, stuck.next_action }
    assert_equal :cancelled, stuck.cancel
    assert_unchanged(stuck) { assert_equal :exhausted, stuck.next_action }
  end

  def test_an_attempt_the_saga_never_issued_is_stale
    near = Saga.with_scope(2, ID, 1)
    far = Saga.with_scope(2, ID, 2)
    from_near = near.next_action
    from_far = far.next_action
    assert_equal from_near.key, from_far.key
    assert_unchanged(far) { assert_equal :stale, far.resolve(from_near, :success) }
    assert_unchanged(near) do
      assert_equal :stale, near.resolve(from_far, :success)
      [(1 << 48) | 2, (2 << 48) | 1, 2**64 - 1].each do |raw|
        assert_equal :stale, near.resolve(from_near.with(attempt: AttemptId.new(raw)), :success)
      end
    end
    assert_equal :resolved, near.resolve(from_near, :success)
  end

  def test_an_attempt_that_is_neither_pending_nor_last_settled_is_stale
    saga = Saga.new(3, ID)
    first = run_step(saga, :success)
    second = run_step(saga, :failure)
    undo = run_step(saga, :failure)
    saga.next_action
    assert_unchanged(saga) do
      assert_equal :stale, saga.resolve(first, :success)
      assert_equal :stale, saga.resolve(first, :failure)
      assert_equal :stale, saga.resolve(second, :failure)
      assert_equal :already_resolved, saga.resolve(undo, :failure)
      assert_equal :mismatch, saga.resolve(undo, :success)
    end
  end

  def test_a_contradicting_report_is_a_mismatch
    saga = Saga.new(2, ID)
    pending = saga.next_action
    keys = [ActionKey.new(ID, 1, :forward), ActionKey.new(ID, 0, :compensate),
            ActionKey.new(SagaId.new(8), 0, :forward)]
    assert_unchanged(saga) do
      keys.each { |key| assert_equal :mismatch, saga.resolve(pending.with(key: key), :success) }
    end
    assert_equal :resolved, saga.resolve(pending, :success)
    assert_unchanged(saga) do
      assert_equal :mismatch, saga.resolve(pending, :failure)
      assert_equal :mismatch, saga.resolve(pending.with(key: keys.first), :success)
    end
  end

  def test_cancel_is_refused_while_a_forward_action_is_pending_and_once_complete
    saga = Saga.new(1, ID)
    action = saga.next_action
    assert_unchanged(saga) { assert_equal :busy, saga.cancel }
    assert_equal :resolved, saga.resolve(action, :success)
    assert_unchanged(saga) { assert_equal :concluded, saga.cancel }
    empty = Saga.new(0, ID)
    assert_unchanged(empty) { assert_equal :concluded, empty.cancel }
  end
end
