# frozen_string_literal: true

require_relative 'test_helper'

# The saga never loses work. `next_action` issues an attempt and answers an
# Action, so the core decides on a copy, the Action is built (SagaId,
# AttemptId, ActionKey, Action), and the copy is kept only if the saga is
# still unfrozen and unchanged. A `new` that raises, calls back in, or
# freezes the saga applies nothing: no attempt is issued that the caller
# never saw.
class WorkTest < Minitest::Test
  include SagaHelpers

  BUILT = [SagaId, AttemptId, ActionKey, Action].freeze

  # Runs the block with `klass.new` replaced by `body`, the way reopening the
  # class could redefine it; the real `new` is `klass.__real_new`.
  def replacing_new(klass, body)
    klass.singleton_class.alias_method(:__real_new, :new)
    klass.define_singleton_method(:new, &body)
    yield
  ensure
    klass.singleton_class.alias_method(:new, :__real_new)
    klass.singleton_class.remove_method(:__real_new)
  end

  # A saga one step in, and a twin given the same history.
  def pair = Array.new(2) { Saga.with_scope(4, ID, 3).tap { run_step(_1, :success) } }

  def test_a_new_that_raises_applies_nothing
    BUILT.each do |klass|
      saga, twin = pair
      replacing_new(klass, ->(*) { raise IOError }) { assert_raises(IOError, klass.name) { saga.next_action } }
      assert_equal state(twin), state(saga), klass.name
      assert_equal twin.next_action, saga.next_action
    end
  end

  def test_a_new_that_calls_back_in_applies_nothing
    BUILT.each do |klass|
      saga, twin = pair
      reenter = lambda do |*args|
        saga.cancel
        klass.__real_new(*args)
      end
      replacing_new(klass, reenter) { assert_raises(RuntimeError, klass.name) { saga.next_action } }
      twin.cancel
      assert_equal state(twin), state(saga), klass.name
      assert_equal twin.next_action, saga.next_action
    end
  end

  def test_a_new_that_freezes_the_saga_applies_nothing
    BUILT.each do |klass|
      saga, twin = pair
      freeze = lambda do |*args|
        saga.freeze
        klass.__real_new(*args)
      end
      replacing_new(klass, freeze) { assert_raises(FrozenError, klass.name) { saga.next_action } }
      assert_equal state(twin), state(saga), klass.name
    end
  end

  def test_every_mutating_call_refuses_a_frozen_saga_up_front
    saga = Saga.new(2, ID)
    action = saga.next_action
    saga.freeze
    assert_unchanged(saga) do
      assert_raises(FrozenError) { saga.next_action }
      assert_raises(FrozenError) { saga.resolve(action, :success) }
      assert_raises(FrozenError) { saga.cancel }
    end
  end
end
