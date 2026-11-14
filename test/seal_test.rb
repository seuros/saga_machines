# frozen_string_literal: true

require_relative 'test_helper'

# The values are sealed, and sealing stops accidents, not reflection: the
# extension is the authority, and every value it reads back must be exactly
# its class and valid by the core, whatever built it.
class SealTest < Minitest::Test
  include SagaHelpers

  VALUES = [SagaId, AttemptId, ActionKey, Action, Forward, Compensating, Settlement, Snapshot].freeze

  # A named subclass raises too, but Ruby binds the constant before
  # `inherited` runs: the class leaks, and can be reopened to skip the check.
  def leak(value)
    name = :"Leak#{value.name.split('::').last}"
    return self.class.const_get(name) if self.class.const_defined?(name, false)

    assert_raises(TypeError) { self.class.class_eval("class #{name} < #{value}; end", __FILE__, __LINE__) }
    self.class.const_get(name).tap do |leaked|
      leaked.class_eval { def initialize(**fields) = Data.instance_method(:initialize).bind_call(self, **fields) }
    end
  end

  def test_values_cannot_be_subclassed_or_allocated
    VALUES.each do |value|
      assert_raises(TypeError) { Class.new(value) { def initialize(**) = nil } }
      assert_operator leak(value), :<, value
      assert_raises(NoMethodError) { value.allocate }
    end
    assert_raises(RangeError) { AttemptId.new(1).with(raw: 0) }
  end

  # A forged value never reaches the core and never earns a refusal symbol
  # (:stale, :mismatch): a leaked subclass is TypeError, a field
  # `Marshal.load` rewrote RangeError.
  def test_forged_values_are_refused_where_the_extension_reads_them
    fake_id = leak(SagaId).new(raw: -1)
    assert_raises(TypeError) { Saga.new(2, fake_id) }
    assert_raises(TypeError) { ActionKey.new(fake_id, 0, :forward) }

    saga = Saga.new(2, ID)
    action = saga.next_action
    fake_attempt = leak(AttemptId).new(raw: 0)
    fake_key = leak(ActionKey).new(saga: ID, step: 0, kind: :forward)
    fake_action = leak(Action).new(key: action.key, attempt: action.attempt)
    zero = Marshal.load(Marshal.dump(action).sub("i\x06", "i\x00"))
    assert_equal 0, zero.attempt.raw
    assert_unchanged(saga) do
      assert_raises(TypeError) { saga.resolve(fake_action, :success) }
      assert_raises(RangeError) { saga.resolve(zero, :success) }
    end
    assert_raises(TypeError) { Action.new(action.key, fake_attempt) }
    assert_raises(TypeError) { Action.new(fake_key, action.attempt) }
    assert_raises(TypeError) { Settlement.new(fake_action, :success) }
    assert_raises(RangeError) { Settlement.new(zero, :success) }
    assert_raises(TypeError) { Snapshot.new(ID, 0, leak(Forward).new(step: 0), 0, nil) }
    assert_equal :resolved, saga.resolve(action, :success)
  end

  def test_a_forged_snapshot_is_refused
    saga = Saga.with_scope(2, ID, 4)
    run_step(saga, :success)
    snapshot = saga.snapshot
    assert_raises(TypeError) { Saga.restored(2, leak(Snapshot).new(**snapshot.to_h), 4) }
    moved = Marshal.load(Marshal.dump(snapshot).sub("i\x09", "i\x0A"))
    assert_equal 5, moved.scope
    assert_raises(RangeError) { Saga.restored(2, moved, 5) }
    assert_equal snapshot, Saga.restored(2, snapshot, 4).snapshot
  end

  # The classes the extension resolved at init are pinned: after a
  # compaction that moves what it can, every path that builds or reads a
  # value still finds them.
  def test_resolved_classes_survive_compaction
    GC.verify_compaction_references(expand_heap: true, toward: :empty) if GC.respond_to?(:verify_compaction_references)
    GC.compact if GC.respond_to?(:compact)
    saga = Saga.with_scope(2, ID, 4)
    action = saga.next_action
    assert_instance_of Action, action
    assert_equal :resolved, saga.resolve(action, :success)
    assert_equal saga.snapshot, Saga.restored(2, saga.snapshot, 4).snapshot
    assert_equal Settlement.new(action, :success), saga.settled
  end
end
